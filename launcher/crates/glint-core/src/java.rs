use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    Error, Result,
    download::{self, Download, Hash, Progress, ProgressFn},
    fs_util,
    paths::Paths,
};

const ADOPTIUM_API: &str = "https://api.adoptium.net/v3";
const MARKER_FILE: &str = "glint-runtime.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaRuntime {
    pub major: u32,
    /// Temurin release name, e.g. `jdk-25.0.5+8`.
    pub release: String,
    /// The `java`/`javaw` executable.
    pub executable: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Marker {
    major: u32,
    release: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumAsset {
    binary: AdoptiumBinary,
    release_name: String,
}

#[derive(Debug, Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(Debug, Deserialize)]
struct AdoptiumPackage {
    checksum: String,
    link: String,
    name: String,
    size: u64,
}

fn runtime_dir(paths: &Paths, major: u32) -> PathBuf {
    paths.java_dir().join(format!("temurin-{major}"))
}

/// `javaw` on Windows so no console window opens; its output can still be piped.
fn find_executable(home: &Path) -> Option<PathBuf> {
    let candidates: &[&str] = if cfg!(windows) {
        &["bin/javaw.exe", "bin/java.exe"]
    } else {
        // macOS archives nest the runtime under Contents/Home.
        &["bin/java", "Contents/Home/bin/java"]
    };
    candidates
        .iter()
        .map(|c| home.join(c))
        .find(|p| p.is_file())
}

/// A runtime Glint installed earlier, if it is complete.
pub async fn find_installed(paths: &Paths, major: u32) -> Option<JavaRuntime> {
    let dir = runtime_dir(paths, major);
    // The marker is written last, so its presence means the unpack finished.
    let marker: Marker = fs_util::read_json_opt(&dir.join(MARKER_FILE))
        .await
        .ok()??;
    Some(JavaRuntime {
        major: marker.major,
        release: marker.release,
        executable: find_executable(&dir)?,
    })
}

/// Return the Temurin runtime for `major`, downloading and unpacking it if needed.
pub async fn ensure(
    client: &reqwest::Client,
    paths: &Paths,
    major: u32,
    progress: &ProgressFn,
) -> Result<JavaRuntime> {
    if let Some(runtime) = find_installed(paths, major).await {
        return Ok(runtime);
    }
    progress(Progress {
        stage: "java",
        done: 0,
        total: 1,
    });

    let asset = latest_asset(client, major).await?;
    let archive = paths.java_dir().join(&asset.binary.package.name);
    let item = Download {
        url: asset.binary.package.link.clone(),
        path: archive.clone(),
        hash: Some(Hash::Sha256(asset.binary.package.checksum.clone())),
        size: Some(asset.binary.package.size),
    };
    if !download::is_valid(&item, true).await {
        download::fetch(client, &item).await?;
    }

    let dir = runtime_dir(paths, major);
    let staging = paths.java_dir().join(format!("temurin-{major}.unpacking"));
    {
        let (archive, staging, dir) = (archive.clone(), staging.clone(), dir.clone());
        tokio::task::spawn_blocking(move || -> Result<()> {
            remove_dir_if_exists(&staging)?;
            remove_dir_if_exists(&dir)?;
            unpack(&archive, &staging)?;
            std::fs::rename(&staging, &dir)?;
            Ok(())
        })
        .await
        .map_err(|e| Error::Java(format!("unpacking Java failed: {e}")))??;
    }
    let _ = tokio::fs::remove_file(&archive).await;

    let executable = find_executable(&dir)
        .ok_or_else(|| Error::Java(format!("the Java {major} download has no java executable")))?;
    let marker = Marker {
        major,
        release: asset.release_name.clone(),
    };
    fs_util::write_json(&dir.join(MARKER_FILE), &marker).await?;
    progress(Progress {
        stage: "java",
        done: 1,
        total: 1,
    });
    Ok(JavaRuntime {
        major,
        release: asset.release_name,
        executable,
    })
}

async fn latest_asset(client: &reqwest::Client, major: u32) -> Result<AdoptiumAsset> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x64"
    };
    let url = format!(
        "{ADOPTIUM_API}/assets/latest/{major}/hotspot?architecture={arch}&image_type=jre&os={os}&vendor=eclipse"
    );
    let assets: Vec<AdoptiumAsset> = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    assets.into_iter().next().ok_or_else(|| {
        Error::Java(format!(
            "Adoptium has no Java {major} runtime for {os}/{arch}"
        ))
    })
}

fn remove_dir_if_exists(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Archives hold a single top-level folder (`jdk-25.0.5+8-jre/...`). Drop it, and refuse
/// any entry that would land outside `dest`.
fn strip_top_level(entry: &Path) -> Option<PathBuf> {
    let mut components = entry.components();
    components.next()?;
    let rest: PathBuf = components.collect();
    if rest.as_os_str().is_empty() {
        return None;
    }
    rest.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then_some(rest)
}

fn unpack(archive: &Path, dest: &Path) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let name = archive.to_string_lossy().to_lowercase();
    if name.ends_with(".zip") {
        unpack_zip(archive, dest)
    } else if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        unpack_tar_gz(archive, dest)
    } else {
        Err(Error::Java(format!(
            "don't know how to unpack {}",
            archive.display()
        )))
    }
}

fn unpack_zip(archive: &Path, dest: &Path) -> Result<()> {
    let zip_error = |e: zip::result::ZipError| Error::Java(format!("bad Java archive: {e}"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(zip_error)?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(zip_error)?;
        let Some(relative) = entry.enclosed_name().and_then(|p| strip_top_level(&p)) else {
            continue;
        };
        let out = dest.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::io::copy(&mut entry, &mut std::fs::File::create(&out)?)?;
    }
    Ok(())
}

fn unpack_tar_gz(archive: &Path, dest: &Path) -> Result<()> {
    let decoder = flate2::read::GzDecoder::new(std::fs::File::open(archive)?);
    let mut tar = tar::Archive::new(decoder);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let Some(relative) = strip_top_level(&entry.path()?) else {
            continue;
        };
        let out = dest.join(relative);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // `unpack` keeps the executable bit and symlinks, which the JRE needs on Unix.
        entry.unpack(&out)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_archive_root_and_rejects_escapes() {
        assert_eq!(
            strip_top_level(Path::new("jdk-25.0.5+8-jre/bin/javaw.exe")),
            Some(PathBuf::from("bin/javaw.exe"))
        );
        assert_eq!(strip_top_level(Path::new("jdk-25.0.5+8-jre")), None);
        assert_eq!(strip_top_level(Path::new("jdk/../../evil")), None);
    }

    #[test]
    fn parses_adoptium_response() {
        let json = include_str!("../tests/fixtures/adoptium-25-windows.json");
        let assets: Vec<AdoptiumAsset> = serde_json::from_str(json).unwrap();
        assert_eq!(assets[0].release_name, "jdk-25.0.5+8");
        assert!(assets[0].binary.package.name.ends_with(".zip"));
        assert_eq!(assets[0].binary.package.checksum.len(), 64);
    }
}

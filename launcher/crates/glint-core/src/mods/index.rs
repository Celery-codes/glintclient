//! What is installed in an instance.
//!
//! The `mods` folder is the truth: a mod is installed if its jar is there. `mods.json`
//! next to it only remembers where each jar came from, so mods can be updated. Jars the
//! user copies in by hand show up too, as untracked entries.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::Source;
use crate::{
    Error, Result,
    download::{self, Hash},
    fs_util,
    paths::Paths,
};

/// Fabric ignores files that don't end in `.jar`, so renaming is how a mod is disabled.
pub const DISABLED_SUFFIX: &str = ".disabled";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledMod {
    /// The jar's name, without the disabled suffix.
    pub filename: String,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    pub title: String,
    #[serde(default)]
    pub version_name: String,
    /// `None` for a jar Glint didn't install and couldn't identify.
    #[serde(default)]
    pub source: Option<Source>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub slug: Option<String>,
    #[serde(default)]
    pub version_id: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub page_url: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    /// When the installed build was published; used to avoid "updating" backwards.
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub size: u64,
}

fn enabled_by_default() -> bool {
    true
}

impl InstalledMod {
    /// Whether this entry is the given project on the given site. `project` may be an
    /// id or a slug, since presets name mods by slug.
    pub fn is_project(&self, source: Source, project: &str) -> bool {
        self.source == Some(source)
            && (self.project_id.as_deref() == Some(project)
                || self
                    .slug
                    .as_deref()
                    .is_some_and(|slug| slug.eq_ignore_ascii_case(project)))
    }
}

#[derive(Default, Serialize, Deserialize)]
struct IndexFile {
    mods: Vec<InstalledMod>,
}

pub fn mods_dir(paths: &Paths, instance_id: &str) -> PathBuf {
    paths.instance_game_dir(instance_id).join("mods")
}

fn index_path(paths: &Paths, instance_id: &str) -> PathBuf {
    paths.instance_dir(instance_id).join("mods.json")
}

/// Where the mod's jar currently is, given its enabled state.
pub fn file_path(paths: &Paths, instance_id: &str, installed: &InstalledMod) -> PathBuf {
    let dir = mods_dir(paths, instance_id);
    if installed.enabled {
        dir.join(&installed.filename)
    } else {
        dir.join(format!("{}{DISABLED_SUFFIX}", installed.filename))
    }
}

/// File names come from remote APIs, so make sure one is a plain jar name before it
/// is joined to a path: no directories, no `..`.
pub fn safe_filename(name: &str) -> Result<String> {
    let plain = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n == name);
    let jar = name.to_ascii_lowercase().ends_with(".jar") && name.len() > 4;
    if plain && jar && !name.contains(['/', '\\', ':']) && !name.chars().any(char::is_control) {
        Ok(name.to_string())
    } else {
        Err(Error::Other(format!(
            "refusing unsafe mod file name: {name:?}"
        )))
    }
}

pub async fn save(paths: &Paths, instance_id: &str, mods: &[InstalledMod]) -> Result<()> {
    let file = IndexFile {
        mods: mods.to_vec(),
    };
    fs_util::write_json(&index_path(paths, instance_id), &file).await
}

/// "sodium-fabric-0.6.13+mc1.21.jar" → "sodium-fabric-0.6.13+mc1.21"
fn title_from_filename(filename: &str) -> String {
    filename
        .strip_suffix(".jar")
        .or_else(|| filename.strip_suffix(".JAR"))
        .unwrap_or(filename)
        .to_string()
}

/// List the instance's mods by reading its `mods` folder and joining what the index
/// knows. Entries for deleted jars are dropped and new jars are added as untracked.
pub async fn scan(paths: &Paths, instance_id: &str) -> Result<Vec<InstalledMod>> {
    let index: IndexFile = fs_util::read_json_opt(&index_path(paths, instance_id))
        .await?
        .unwrap_or_default();
    let mut known: HashMap<String, InstalledMod> = index
        .mods
        .iter()
        .cloned()
        .map(|m| (m.filename.clone(), m))
        .collect();

    // filename → (enabled, size). If a jar exists both enabled and disabled, the
    // enabled copy is the one the game loads, so it wins.
    let mut on_disk: HashMap<String, (bool, u64)> = HashMap::new();
    let dir = mods_dir(paths, instance_id);
    match tokio::fs::read_dir(&dir).await {
        Ok(mut entries) => {
            while let Some(entry) = entries.next_entry().await? {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                let (filename, enabled) = match name.strip_suffix(DISABLED_SUFFIX) {
                    Some(base) => (base.to_string(), false),
                    None => (name, true),
                };
                if !filename.to_ascii_lowercase().ends_with(".jar") {
                    continue;
                }
                let metadata = entry.metadata().await?;
                if !metadata.is_file() {
                    continue;
                }
                let slot = on_disk.entry(filename).or_insert((enabled, metadata.len()));
                if enabled {
                    *slot = (true, metadata.len());
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }

    let mut mods = Vec::with_capacity(on_disk.len());
    for (filename, (enabled, size)) in on_disk {
        let installed = match known.remove(&filename) {
            Some(existing) => InstalledMod {
                enabled,
                size,
                ..existing
            },
            None => {
                let mut untracked = InstalledMod {
                    title: title_from_filename(&filename),
                    filename,
                    enabled,
                    size,
                    version_name: String::new(),
                    source: None,
                    project_id: None,
                    slug: None,
                    version_id: None,
                    icon_url: None,
                    page_url: None,
                    sha1: None,
                    published: None,
                };
                // Hashed once, when the jar is first seen, so it can be looked up later.
                let path = file_path(paths, instance_id, &untracked);
                untracked.sha1 = download::hash_file(&path, &Hash::Sha1(String::new()))
                    .await
                    .ok();
                untracked
            }
        };
        mods.push(installed);
    }
    mods.sort_by_cached_key(|m| (m.title.to_lowercase(), m.filename.clone()));

    if mods != index.mods {
        save(paths, instance_id, &mods).await?;
    }
    Ok(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filenames_are_checked() {
        assert!(safe_filename("sodium-fabric-0.6.13+mc1.21.jar").is_ok());
        assert!(safe_filename("Mod Name [Fabric] 1.0.JAR").is_ok());
        for bad in [
            "../evil.jar",
            "..\\evil.jar",
            "sub/mod.jar",
            "C:evil.jar",
            "mod.exe",
            "mod.jar.disabled",
            ".jar",
            "",
        ] {
            assert!(safe_filename(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[tokio::test]
    async fn scan_tracks_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        assert!(scan(&paths, "inst").await.unwrap().is_empty());

        let mods = mods_dir(&paths, "inst");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("b-mod.jar"), b"bbb").unwrap();
        std::fs::write(mods.join("a-mod.jar.disabled"), b"a").unwrap();
        std::fs::write(mods.join("notes.txt"), b"not a mod").unwrap();

        let found = scan(&paths, "inst").await.unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(
            (found[0].filename.as_str(), found[0].enabled),
            ("a-mod.jar", false)
        );
        assert_eq!(
            (found[1].filename.as_str(), found[1].enabled, found[1].size),
            ("b-mod.jar", true, 3)
        );
        assert_eq!(found[1].title, "b-mod");
        assert_eq!(found[1].sha1.as_deref().map(str::len), Some(40));

        // Metadata recorded in the index survives rescans and state changes on disk.
        let mut tracked = found.clone();
        tracked[1].title = "B Mod".into();
        tracked[1].source = Some(Source::Modrinth);
        tracked[1].project_id = Some("proj".into());
        save(&paths, "inst", &tracked).await.unwrap();
        std::fs::rename(mods.join("b-mod.jar"), mods.join("b-mod.jar.disabled")).unwrap();
        std::fs::remove_file(mods.join("a-mod.jar.disabled")).unwrap();

        let found = scan(&paths, "inst").await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "B Mod");
        assert!(!found[0].enabled);
        assert!(found[0].is_project(Source::Modrinth, "proj"));
        assert!(!found[0].is_project(Source::Curseforge, "proj"));
    }
}

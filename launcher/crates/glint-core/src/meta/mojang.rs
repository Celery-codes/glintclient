use serde::{Deserialize, Serialize};

use super::rules::{Env, Rule, allows};
use crate::{
    Error, Result,
    download::{self, Download, Hash},
    fs_util,
    paths::Paths,
};

pub const MANIFEST_URL: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const LIBRARIES_URL: &str = "https://libraries.minecraft.net/";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net/";

/// Release date of 1.19, the first version whose natives are plain rule-filtered
/// libraries. Older versions need native extraction, which Glint doesn't implement.
pub const OLDEST_SUPPORTED_RELEASE_TIME: &str = "2022-06-07";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
    pub release_time: String,
}

impl ManifestEntry {
    pub fn is_supported(&self) -> bool {
        self.release_time.as_str() >= OLDEST_SUPPORTED_RELEASE_TIME
    }
}

impl VersionManifest {
    pub fn find(&self, id: &str) -> Result<&ManifestEntry> {
        self.versions
            .iter()
            .find(|v| v.id == id)
            .ok_or_else(|| Error::NotFound(format!("Minecraft version {id}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub main_class: String,
    #[serde(default)]
    pub inherits_from: Option<String>,
    #[serde(default)]
    pub arguments: Option<Arguments>,
    pub asset_index: AssetIndexRef,
    pub downloads: VersionDownloads,
    #[serde(default)]
    pub java_version: Option<JavaVersion>,
    pub libraries: Vec<Library>,
}

impl VersionInfo {
    /// The vanilla version whose jar and assets this version runs on.
    pub fn vanilla_id(&self) -> &str {
        self.inherits_from.as_deref().unwrap_or(&self.id)
    }

    /// Java 8 is what Mojang shipped before versions started declaring a runtime.
    pub fn java_major(&self) -> u32 {
        self.java_version.as_ref().map_or(8, |j| j.major_version)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl Argument {
    /// The raw (unsubstituted) values this argument contributes on `env`.
    pub fn values(&self, env: &Env) -> Vec<&str> {
        match self {
            Argument::Plain(value) => vec![value],
            Argument::Conditional { rules, value } => {
                if !allows(Some(rules), env) {
                    return Vec::new();
                }
                match value {
                    OneOrMany::One(v) => vec![v],
                    OneOrMany::Many(vs) => vs.iter().map(String::as_str).collect(),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: FileRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRef {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

/// A library entry, in either Mojang's form (`downloads.artifact`) or the Maven form
/// that Fabric uses (`name` plus a repository `url`, with optional top-level hash).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Pre-1.19 native classifier map. Its presence marks an unsupported version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    pub url: String,
}

/// A library reduced to what the installer and classpath builder need.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLibrary {
    /// Path relative to the libraries directory.
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Parsed `group:artifact:version[:classifier][@extension]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MavenCoord {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub classifier: Option<String>,
    pub extension: String,
}

impl MavenCoord {
    pub fn parse(name: &str) -> Option<Self> {
        let (coords, extension) = name.split_once('@').unwrap_or((name, "jar"));
        let mut parts = coords.split(':');
        let coord = Self {
            group: parts.next()?.to_string(),
            artifact: parts.next()?.to_string(),
            version: parts.next()?.to_string(),
            classifier: parts.next().map(str::to_string),
            extension: extension.to_string(),
        };
        (parts.next().is_none() && !coord.group.is_empty()).then_some(coord)
    }

    pub fn path(&self) -> String {
        let classifier = self
            .classifier
            .as_ref()
            .map(|c| format!("-{c}"))
            .unwrap_or_default();
        format!(
            "{}/{}/{}/{}-{}{}.{}",
            self.group.replace('.', "/"),
            self.artifact,
            self.version,
            self.artifact,
            self.version,
            classifier,
            self.extension
        )
    }

    /// Identity ignoring the version, used to let a mod loader's copy of a library
    /// replace the game's.
    pub fn key(&self) -> String {
        match &self.classifier {
            Some(c) => format!("{}:{}:{}", self.group, self.artifact, c),
            None => format!("{}:{}", self.group, self.artifact),
        }
    }
}

impl Library {
    pub fn key(&self) -> String {
        MavenCoord::parse(&self.name).map_or_else(|| self.name.clone(), |c| c.key())
    }

    pub fn resolve(&self) -> Result<ResolvedLibrary> {
        let coord = MavenCoord::parse(&self.name);
        if let Some(artifact) = self.downloads.as_ref().and_then(|d| d.artifact.as_ref()) {
            let path = artifact
                .path
                .clone()
                .or_else(|| coord.as_ref().map(MavenCoord::path))
                .ok_or_else(|| Error::Other(format!("library {} has no path", self.name)))?;
            return Ok(ResolvedLibrary {
                path,
                url: artifact.url.clone(),
                sha1: artifact.sha1.clone(),
                size: artifact.size,
            });
        }
        let coord = coord.ok_or_else(|| Error::Other(format!("bad library name {}", self.name)))?;
        let path = coord.path();
        let base = self.url.as_deref().unwrap_or(LIBRARIES_URL);
        let separator = if base.ends_with('/') { "" } else { "/" };
        Ok(ResolvedLibrary {
            url: format!("{base}{separator}{path}"),
            path,
            sha1: self.sha1.clone(),
            size: self.size,
        })
    }
}

/// Libraries that apply on `env`, in classpath order.
pub fn resolve_libraries(version: &VersionInfo, env: &Env) -> Result<Vec<ResolvedLibrary>> {
    let mut resolved = Vec::new();
    for library in &version.libraries {
        if !allows(library.rules.as_deref(), env) {
            continue;
        }
        if library.natives.is_some() {
            return Err(Error::Launch(format!(
                "Minecraft {} uses the pre-1.19 native library format, which Glint doesn't support",
                version.vanilla_id()
            )));
        }
        resolved.push(library.resolve()?);
    }
    Ok(resolved)
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: std::collections::HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

pub async fn version_manifest(client: &reqwest::Client, paths: &Paths) -> Result<VersionManifest> {
    let cache = paths.meta_dir().join("version_manifest_v2.json");
    super::fetch_json_cached(client, MANIFEST_URL, &cache).await
}

/// The version JSON, downloaded once and then served from disk. The manifest's SHA-1
/// tells us when Mojang has republished it.
pub async fn version_info(
    client: &reqwest::Client,
    paths: &Paths,
    entry: &ManifestEntry,
) -> Result<VersionInfo> {
    let path = paths
        .version_dir(&entry.id)
        .join(format!("{}.json", entry.id));
    let item = Download {
        url: entry.url.clone(),
        path: path.clone(),
        hash: Some(Hash::Sha1(entry.sha1.clone())),
        size: None,
    };
    if !download::is_valid(&item, true).await {
        download::fetch(client, &item).await?;
    }
    fs_util::read_json(&path).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maven_paths() {
        let c = MavenCoord::parse("net.fabricmc:fabric-loader:0.20.1").unwrap();
        assert_eq!(
            c.path(),
            "net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
        );
        assert_eq!(c.key(), "net.fabricmc:fabric-loader");

        let c = MavenCoord::parse("org.lwjgl:lwjgl-sdl:3.4.3:natives-windows").unwrap();
        assert_eq!(
            c.path(),
            "org/lwjgl/lwjgl-sdl/3.4.3/lwjgl-sdl-3.4.3-natives-windows.jar"
        );
        assert_eq!(c.key(), "org.lwjgl:lwjgl-sdl:natives-windows");

        let c = MavenCoord::parse("a.b:c:1.0@zip").unwrap();
        assert_eq!(c.path(), "a/b/c/1.0/c-1.0.zip");

        assert!(MavenCoord::parse("nonsense").is_none());
        assert!(MavenCoord::parse("a:b:c:d:e").is_none());
    }

    #[test]
    fn fabric_style_library_resolves_against_its_repo() {
        let lib: Library = serde_json::from_str(
            r#"{"name":"org.ow2.asm:asm:9.10.1","url":"https://maven.fabricmc.net/","sha1":"abc","size":10}"#,
        )
        .unwrap();
        let r = lib.resolve().unwrap();
        assert_eq!(
            r.url,
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.10.1/asm-9.10.1.jar"
        );
        assert_eq!(r.sha1.as_deref(), Some("abc"));
        assert_eq!(r.size, Some(10));
    }
}

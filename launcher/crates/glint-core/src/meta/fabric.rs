use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::mojang::{Arguments, Library, VersionInfo};
use crate::{Result, fs_util, paths::Paths};

pub const META_URL: &str = "https://meta.fabricmc.net/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderVersion,
}

/// The launcher profile Fabric publishes for a game + loader pair. It only holds what
/// Fabric adds; everything else is inherited from the vanilla version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricProfile {
    pub id: String,
    pub inherits_from: String,
    pub main_class: String,
    #[serde(default)]
    pub arguments: Arguments,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

/// Minecraft versions Fabric supports, newest first.
pub async fn game_versions(client: &reqwest::Client, paths: &Paths) -> Result<Vec<GameVersion>> {
    let cache = paths.meta_dir().join("fabric-game-versions.json");
    super::fetch_json_cached(client, &format!("{META_URL}/versions/game"), &cache).await
}

/// Loader versions available for a Minecraft version, newest first. Empty if Fabric
/// doesn't support that version.
pub async fn loader_versions(
    client: &reqwest::Client,
    paths: &Paths,
    mc_version: &str,
) -> Result<Vec<LoaderVersion>> {
    let cache = paths
        .meta_dir()
        .join(format!("fabric-loaders-{mc_version}.json"));
    let entries: Vec<LoaderEntry> = super::fetch_json_cached(
        client,
        &format!("{META_URL}/versions/loader/{mc_version}"),
        &cache,
    )
    .await?;
    Ok(entries.into_iter().map(|e| e.loader).collect())
}

/// Newest stable loader, or the newest of any kind if none is marked stable.
pub fn recommended_loader(versions: &[LoaderVersion]) -> Option<&LoaderVersion> {
    versions.iter().find(|v| v.stable).or(versions.first())
}

/// A profile never changes for a given pair, so it's fetched once and kept on disk.
pub async fn profile(
    client: &reqwest::Client,
    paths: &Paths,
    mc_version: &str,
    loader_version: &str,
) -> Result<FabricProfile> {
    let id = format!("fabric-loader-{loader_version}-{mc_version}");
    let path = paths.version_dir(&id).join(format!("{id}.json"));
    if let Some(profile) = fs_util::read_json_opt(&path).await? {
        return Ok(profile);
    }
    let url = format!("{META_URL}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let profile: FabricProfile = serde_json::from_slice(&bytes)?;
    fs_util::write_atomic(&path, &bytes).await?;
    Ok(profile)
}

/// Layer a Fabric profile over the vanilla version it inherits from.
///
/// Fabric's libraries go first on the classpath and replace any vanilla library with the
/// same group and artifact. Its arguments are appended, and its main class takes over.
pub fn merge(vanilla: VersionInfo, profile: FabricProfile) -> VersionInfo {
    let mut seen = HashSet::new();
    let mut libraries = Vec::with_capacity(profile.libraries.len() + vanilla.libraries.len());
    for library in profile.libraries.into_iter().chain(vanilla.libraries) {
        // Rule-filtered duplicates (per-OS natives) share a key by design; only
        // unconditional libraries are deduplicated.
        if library.rules.is_none() && !seen.insert(library.key()) {
            continue;
        }
        libraries.push(library);
    }

    let mut arguments = vanilla.arguments.unwrap_or_default();
    arguments.game.extend(profile.arguments.game);
    arguments.jvm.extend(profile.arguments.jvm);

    VersionInfo {
        id: profile.id,
        inherits_from: Some(vanilla.id),
        main_class: profile.main_class,
        arguments: Some(arguments),
        libraries,
        ..vanilla
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_stable_loader() {
        let v = |version: &str, stable| LoaderVersion {
            version: version.into(),
            stable,
        };
        let list = [
            v("0.21.0-beta.1", false),
            v("0.20.1", true),
            v("0.20.0", false),
        ];
        assert_eq!(recommended_loader(&list).unwrap().version, "0.20.1");
        let unstable = [v("0.21.0-beta.1", false)];
        assert_eq!(
            recommended_loader(&unstable).unwrap().version,
            "0.21.0-beta.1"
        );
        assert!(recommended_loader(&[]).is_none());
    }
}

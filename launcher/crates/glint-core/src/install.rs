use std::{collections::HashSet, path::PathBuf};

use crate::{
    Result,
    download::{self, Download, Hash, ProgressFn},
    fs_util,
    meta::{
        mojang::{self, AssetIndex, VersionInfo},
        rules::Env,
    },
    paths::Paths,
};

/// What a finished install hands to the launcher.
#[derive(Debug, Clone)]
pub struct Installed {
    /// Libraries in classpath order, followed by the client jar.
    pub classpath: Vec<PathBuf>,
}

/// Download everything `version` needs: client jar, libraries and assets.
///
/// Safe to call before every launch: files that are already present are skipped.
pub async fn install(
    client: &reqwest::Client,
    paths: &Paths,
    version: &VersionInfo,
    env: &Env,
    concurrency: usize,
    progress: &ProgressFn,
) -> Result<Installed> {
    let vanilla_id = version.vanilla_id();
    let client_jar = paths
        .version_dir(vanilla_id)
        .join(format!("{vanilla_id}.jar"));

    let libraries = mojang::resolve_libraries(version, env)?;
    let libraries_dir = paths.libraries_dir();
    let mut classpath = Vec::with_capacity(libraries.len() + 1);
    let mut library_downloads = Vec::with_capacity(libraries.len() + 1);
    for library in libraries {
        let path = libraries_dir.join(&library.path);
        classpath.push(path.clone());
        library_downloads.push(Download {
            url: library.url,
            path,
            hash: library.sha1.map(Hash::Sha1),
            size: library.size,
        });
    }
    classpath.push(client_jar.clone());
    library_downloads.push(Download {
        url: version.downloads.client.url.clone(),
        path: client_jar,
        hash: Some(Hash::Sha1(version.downloads.client.sha1.clone())),
        size: Some(version.downloads.client.size),
    });
    download::fetch_all(
        client,
        library_downloads,
        concurrency,
        "libraries",
        progress,
    )
    .await?;

    let asset_downloads = asset_downloads(client, paths, version).await?;
    download::fetch_all(client, asset_downloads, concurrency, "assets", progress).await?;

    Ok(Installed { classpath })
}

async fn asset_downloads(
    client: &reqwest::Client,
    paths: &Paths,
    version: &VersionInfo,
) -> Result<Vec<Download>> {
    let index_ref = &version.asset_index;
    let index_item = Download {
        url: index_ref.url.clone(),
        path: paths
            .assets_dir()
            .join("indexes")
            .join(format!("{}.json", index_ref.id)),
        hash: Some(Hash::Sha1(index_ref.sha1.clone())),
        size: Some(index_ref.size),
    };
    if !download::is_valid(&index_item, false).await {
        download::fetch(client, &index_item).await?;
    }
    let index: AssetIndex = fs_util::read_json(&index_item.path).await?;

    // Many asset paths share one object (same hash), so download each hash once.
    let objects_dir = paths.assets_dir().join("objects");
    let mut seen = HashSet::new();
    let mut downloads = Vec::new();
    for object in index.objects.into_values() {
        if object.hash.len() < 2 || !seen.insert(object.hash.clone()) {
            continue;
        }
        let prefix = &object.hash[..2];
        downloads.push(Download {
            url: format!("{}{prefix}/{}", mojang::RESOURCES_URL, object.hash),
            path: objects_dir.join(prefix).join(&object.hash),
            hash: Some(Hash::Sha1(object.hash.clone())),
            size: Some(object.size),
        });
    }
    Ok(downloads)
}

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;

use super::{
    Dependency, LOADER, ModFile, ProjectInfo, ReleaseType, SearchHit, SearchPage, SearchQuery,
    Source, find_category,
};
use crate::{Error, Result};

const API: &str = "https://api.modrinth.com/v2";

fn page_url(slug: &str) -> String {
    format!("https://modrinth.com/mod/{slug}")
}

#[derive(Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
    total_hits: u64,
}

#[derive(Deserialize)]
struct Hit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    #[serde(default)]
    icon_url: Option<String>,
    downloads: u64,
}

#[derive(Deserialize)]
struct Version {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    version_type: String,
    date_published: String,
    #[serde(default)]
    game_versions: Vec<String>,
    files: Vec<VersionFile>,
    #[serde(default)]
    dependencies: Vec<VersionDependency>,
}

#[derive(Deserialize)]
struct VersionFile {
    hashes: Hashes,
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    size: u64,
}

#[derive(Deserialize)]
struct Hashes {
    #[serde(default)]
    sha1: Option<String>,
}

#[derive(Deserialize)]
struct VersionDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    dependency_type: String,
}

#[derive(Deserialize)]
struct Project {
    id: String,
    slug: String,
    title: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    loaders: Vec<String>,
}

impl Version {
    /// `None` for a version with no files, which Modrinth allows but can't be installed.
    fn into_file(self) -> Option<ModFile> {
        let index = self.files.iter().position(|f| f.primary).unwrap_or(0);
        let file = self.files.into_iter().nth(index)?;
        // The version number is what users recognise ("0.6.13"); fall back to the
        // display name for the rare version without one.
        let version_name = if self.version_number.is_empty() {
            self.name
        } else {
            self.version_number
        };
        Some(ModFile {
            source: Source::Modrinth,
            project_id: self.project_id,
            version_id: self.id,
            version_name,
            filename: file.filename,
            url: Some(file.url),
            sha1: file.hashes.sha1,
            size: file.size,
            release_type: match self.version_type.as_str() {
                "release" => ReleaseType::Release,
                "beta" => ReleaseType::Beta,
                _ => ReleaseType::Alpha,
            },
            published: self.date_published,
            game_versions: self.game_versions,
            dependencies: self
                .dependencies
                .into_iter()
                .filter(|d| d.dependency_type == "required")
                .filter_map(|d| {
                    Some(Dependency {
                        project_id: d.project_id?,
                        version_id: d.version_id,
                    })
                })
                .collect(),
        })
    }
}

/// Modrinth filters with "facets": a list of AND-ed groups, each a list of OR-ed terms.
fn facets(query: &SearchQuery) -> String {
    let mut groups = vec![vec!["project_type:mod".to_string()]];
    if query.fabric_only {
        groups.push(vec![format!("categories:{LOADER}")]);
    }
    if let Some(version) = &query.mc_version {
        groups.push(vec![format!("versions:{version}")]);
    }
    if let Some(category) = query.category.as_deref().and_then(find_category) {
        groups.push(vec![format!("categories:{}", category.modrinth)]);
    }
    serde_json::to_string(&groups).unwrap_or_default()
}

pub async fn search(client: &reqwest::Client, query: &SearchQuery) -> Result<SearchPage> {
    let response: SearchResponse = client
        .get(format!("{API}/search"))
        .query(&[
            ("query", query.text.as_str()),
            ("facets", &facets(query)),
            ("limit", &query.page_size.to_string()),
            ("offset", &(query.page * query.page_size).to_string()),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(SearchPage {
        total: response.total_hits,
        hits: response
            .hits
            .into_iter()
            .map(|hit| SearchHit {
                source: Source::Modrinth,
                page_url: page_url(&hit.slug),
                project_id: hit.project_id,
                slug: hit.slug,
                title: hit.title,
                description: hit.description,
                author: hit.author,
                icon_url: hit.icon_url,
                downloads: hit.downloads,
                downloadable: true,
            })
            .collect(),
    })
}

/// Builds of a project (by id or slug) for a Minecraft version on Fabric.
pub async fn files(
    client: &reqwest::Client,
    project: &str,
    mc_version: &str,
) -> Result<Vec<ModFile>> {
    let response = client
        .get(format!("{API}/project/{project}/version"))
        .query(&[
            ("loaders", json!([LOADER]).to_string()),
            ("game_versions", json!([mc_version]).to_string()),
        ])
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::NotFound(format!("Modrinth project {project}")));
    }
    let versions: Vec<Version> = response.error_for_status()?.json().await?;
    Ok(versions
        .into_iter()
        .filter_map(Version::into_file)
        .collect())
}

pub async fn file(client: &reqwest::Client, version_id: &str) -> Result<Option<ModFile>> {
    let response = client
        .get(format!("{API}/version/{version_id}"))
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let version: Version = response.error_for_status()?.json().await?;
    Ok(version.into_file())
}

/// Look up projects by id or slug. Unknown ones are simply absent from the result.
pub async fn projects(client: &reqwest::Client, ids: &[String]) -> Result<Vec<ProjectInfo>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let projects: Vec<Project> = client
        .get(format!("{API}/projects"))
        .query(&[("ids", json!(ids).to_string())])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(projects
        .into_iter()
        .map(|p| ProjectInfo {
            page_url: page_url(&p.slug),
            project_id: p.id,
            slug: p.slug,
            title: p.title,
            icon_url: p.icon_url,
            loaders: p.loaders,
        })
        .collect())
}

/// Recognise jar files by SHA-1, e.g. mods the user copied in by hand. Returns the
/// matching build for each hash Modrinth knows.
pub async fn identify(
    client: &reqwest::Client,
    sha1s: &[String],
) -> Result<HashMap<String, ModFile>> {
    if sha1s.is_empty() {
        return Ok(HashMap::new());
    }
    let found: HashMap<String, Version> = client
        .post(format!("{API}/version_files"))
        .json(&json!({ "hashes": sha1s, "algorithm": "sha1" }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(found
        .into_iter()
        .filter_map(|(hash, version)| Some((hash, version.into_file()?)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::pick_file;

    #[test]
    fn parses_search_results() {
        let response: SearchResponse = serde_json::from_str(include_str!(
            "../../tests/fixtures/modrinth-search-sodium.json"
        ))
        .unwrap();
        assert!(response.total_hits > 0);
        assert_eq!(response.hits[0].slug, "sodium");
        assert_eq!(response.hits[0].project_id, "AANobbMI");
    }

    #[test]
    fn parses_versions_and_picks_the_release() {
        let versions: Vec<Version> = serde_json::from_str(include_str!(
            "../../tests/fixtures/modrinth-versions-sodium-26.3.json"
        ))
        .unwrap();
        let files: Vec<ModFile> = versions
            .into_iter()
            .filter_map(Version::into_file)
            .collect();
        assert_eq!(files.len(), 6);
        assert!(
            files[0]
                .url
                .as_deref()
                .unwrap()
                .starts_with("https://cdn.modrinth.com/")
        );
        assert_eq!(files[0].sha1.as_deref().map(str::len), Some(40));
        // Five newer pre-releases exist; the one stable build still wins.
        let picked = pick_file(&files).unwrap();
        assert_eq!(picked.version_name, "mc26.3-0.9.2-fabric");
        assert_eq!(picked.release_type, ReleaseType::Release);
    }

    #[test]
    fn only_required_dependencies_are_kept() {
        let json = r#"{"id":"v","project_id":"p","name":"n","version_number":"1.0","version_type":"release",
            "date_published":"2026-01-01T00:00:00Z","game_versions":["26.3"],
            "files":[{"hashes":{"sha1":"a","sha512":"b"},"url":"u","filename":"other.jar","primary":false,"size":1},
                     {"hashes":{"sha1":"c","sha512":"d"},"url":"u2","filename":"main.jar","primary":true,"size":2}],
            "dependencies":[{"project_id":"req","version_id":null,"dependency_type":"required"},
                            {"project_id":"opt","version_id":null,"dependency_type":"optional"},
                            {"project_id":null,"version_id":"x","dependency_type":"embedded"},
                            {"project_id":"bad","version_id":null,"dependency_type":"incompatible"}]}"#;
        let file = serde_json::from_str::<Version>(json)
            .unwrap()
            .into_file()
            .unwrap();
        assert_eq!(file.filename, "main.jar");
        assert_eq!(
            file.dependencies,
            [Dependency {
                project_id: "req".into(),
                version_id: None
            }]
        );
    }

    #[test]
    fn facets_combine_filters() {
        let query = SearchQuery {
            mc_version: Some("26.3".into()),
            fabric_only: true,
            category: Some("optimization".into()),
            ..Default::default()
        };
        assert_eq!(
            facets(&query),
            r#"[["project_type:mod"],["categories:fabric"],["versions:26.3"],["categories:optimization"]]"#
        );
        assert_eq!(facets(&SearchQuery::default()), r#"[["project_type:mod"]]"#);
    }
}

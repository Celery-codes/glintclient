//! CurseForge client. Every call needs an API key, which comes from configuration.

use serde::Deserialize;
use serde_json::json;

use super::{
    Dependency, ModFile, ProjectInfo, ReleaseType, SearchHit, SearchPage, SearchQuery, Source,
    find_category,
};
use crate::{Error, Result};

const API: &str = "https://api.curseforge.com/v1";
const GAME_MINECRAFT: u32 = 432;
/// The "Mods" section of Minecraft (as opposed to modpacks, resource packs, ...).
const CLASS_MODS: u32 = 6;
const LOADER_FABRIC: u32 = 4;
const SORT_POPULARITY: u32 = 2;
const RELATION_REQUIRED: u32 = 3;
const HASH_SHA1: u32 = 1;

pub struct CurseForge<'a> {
    pub client: &'a reqwest::Client,
    pub api_key: &'a str,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pagination {
    total_count: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Mod {
    id: u64,
    name: String,
    slug: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    download_count: f64,
    #[serde(default)]
    links: Option<Links>,
    #[serde(default)]
    logo: Option<Logo>,
    #[serde(default)]
    authors: Vec<Author>,
    /// `false` means the author has disabled third-party downloads. Absent or null is
    /// treated as allowed.
    #[serde(default)]
    allow_mod_distribution: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Links {
    #[serde(default)]
    website_url: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Logo {
    #[serde(default)]
    thumbnail_url: Option<String>,
}

#[derive(Deserialize)]
struct Author {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct File {
    id: u64,
    mod_id: u64,
    #[serde(default)]
    display_name: String,
    file_name: String,
    #[serde(default)]
    release_type: u32,
    #[serde(default)]
    hashes: Vec<FileHash>,
    #[serde(default)]
    file_date: String,
    #[serde(default)]
    file_length: u64,
    #[serde(default)]
    download_url: Option<String>,
    #[serde(default)]
    game_versions: Vec<String>,
    #[serde(default)]
    dependencies: Vec<FileDependency>,
}

#[derive(Deserialize)]
struct FileHash {
    value: String,
    algo: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileDependency {
    mod_id: u64,
    relation_type: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CfCategory {
    id: u64,
    name: String,
    #[serde(default)]
    class_id: Option<u64>,
    #[serde(default)]
    parent_category_id: Option<u64>,
}

impl Mod {
    fn page_url(&self) -> String {
        self.links
            .as_ref()
            .and_then(|l| l.website_url.clone())
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| {
                format!("https://www.curseforge.com/minecraft/mc-mods/{}", self.slug)
            })
    }

    fn icon_url(&self) -> Option<String> {
        self.logo
            .as_ref()
            .and_then(|l| l.thumbnail_url.clone())
            .filter(|url| !url.is_empty())
    }

    fn into_hit(self) -> SearchHit {
        SearchHit {
            source: Source::Curseforge,
            page_url: self.page_url(),
            icon_url: self.icon_url(),
            downloadable: self.allow_mod_distribution != Some(false),
            project_id: self.id.to_string(),
            slug: self.slug,
            title: self.name,
            description: self.summary,
            author: self
                .authors
                .into_iter()
                .next()
                .map(|a| a.name)
                .unwrap_or_default(),
            downloads: self.download_count as u64,
        }
    }

    fn into_info(self) -> ProjectInfo {
        ProjectInfo {
            page_url: self.page_url(),
            icon_url: self.icon_url(),
            project_id: self.id.to_string(),
            slug: self.slug,
            title: self.name,
            loaders: Vec::new(),
        }
    }
}

impl File {
    fn into_file(self) -> ModFile {
        ModFile {
            source: Source::Curseforge,
            project_id: self.mod_id.to_string(),
            version_id: self.id.to_string(),
            version_name: if self.display_name.is_empty() {
                self.file_name.clone()
            } else {
                self.display_name
            },
            filename: self.file_name,
            // Null or empty when the mod's author has disabled third-party downloads.
            url: self.download_url.filter(|url| !url.is_empty()),
            sha1: self
                .hashes
                .into_iter()
                .find(|h| h.algo == HASH_SHA1)
                .map(|h| h.value),
            size: self.file_length,
            release_type: match self.release_type {
                1 => ReleaseType::Release,
                2 => ReleaseType::Beta,
                _ => ReleaseType::Alpha,
            },
            published: self.file_date,
            game_versions: self.game_versions,
            dependencies: self
                .dependencies
                .into_iter()
                .filter(|d| d.relation_type == RELATION_REQUIRED)
                .map(|d| Dependency {
                    project_id: d.mod_id.to_string(),
                    version_id: None,
                })
                .collect(),
        }
    }
}

impl CurseForge<'_> {
    async fn check(response: reqwest::Response) -> Result<reqwest::Response> {
        match response.status().as_u16() {
            401 | 403 => Err(Error::Config(
                "CurseForge rejected the API key. Check it in Settings.".into(),
            )),
            _ => Ok(response.error_for_status()?),
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Envelope<T>> {
        let response = self
            .client
            .get(format!("{API}{path}"))
            .header("x-api-key", self.api_key)
            .header("Accept", "application/json")
            .query(query)
            .send()
            .await?;
        Ok(Self::check(response).await?.json().await?)
    }

    /// The id of the CurseForge category matching one of Glint's shared categories.
    /// `Ok(None)` if CurseForge has no equivalent.
    async fn category_id(&self, category: &str) -> Result<Option<u64>> {
        let Some(keyword) = find_category(category).and_then(|c| c.curseforge) else {
            return Ok(None);
        };
        let categories: Envelope<Vec<CfCategory>> = self
            .get(
                "/categories",
                &[
                    ("gameId", GAME_MINECRAFT.to_string()),
                    ("classId", CLASS_MODS.to_string()),
                ],
            )
            .await?;
        Ok(match_category(&categories.data, keyword))
    }

    /// `Ok(None)` when the query asks for a category CurseForge doesn't have: there
    /// is nothing meaningful to show for it.
    pub async fn search(&self, query: &SearchQuery) -> Result<Option<SearchPage>> {
        let mut params = vec![
            ("gameId", GAME_MINECRAFT.to_string()),
            ("classId", CLASS_MODS.to_string()),
            ("searchFilter", query.text.clone()),
            ("sortField", SORT_POPULARITY.to_string()),
            ("sortOrder", "desc".to_string()),
            ("index", (query.page * query.page_size).to_string()),
            ("pageSize", query.page_size.min(50).to_string()),
        ];
        if let Some(version) = &query.mc_version {
            params.push(("gameVersion", version.clone()));
            // The API only honours a loader filter together with a game version.
            if query.fabric_only {
                params.push(("modLoaderType", LOADER_FABRIC.to_string()));
            }
        }
        if let Some(category) = &query.category {
            match self.category_id(category).await? {
                Some(id) => params.push(("categoryId", id.to_string())),
                None => return Ok(None),
            }
        }
        let response: Envelope<Vec<Mod>> = self.get("/mods/search", &params).await?;
        Ok(Some(SearchPage {
            total: response.pagination.map_or(0, |p| p.total_count),
            hits: response.data.into_iter().map(Mod::into_hit).collect(),
        }))
    }

    pub async fn files(&self, mod_id: &str, mc_version: &str) -> Result<Vec<ModFile>> {
        let response: Envelope<Vec<File>> = self
            .get(
                &format!("/mods/{mod_id}/files"),
                &[
                    ("gameVersion", mc_version.to_string()),
                    ("modLoaderType", LOADER_FABRIC.to_string()),
                    ("pageSize", "50".to_string()),
                ],
            )
            .await?;
        Ok(response.data.into_iter().map(File::into_file).collect())
    }

    pub async fn projects(&self, ids: &[String]) -> Result<Vec<ProjectInfo>> {
        let ids: Vec<u64> = ids.iter().filter_map(|id| id.parse().ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let response = self
            .client
            .post(format!("{API}/mods"))
            .header("x-api-key", self.api_key)
            .header("Accept", "application/json")
            .json(&json!({ "modIds": ids }))
            .send()
            .await?;
        let mods: Envelope<Vec<Mod>> = Self::check(response).await?.json().await?;
        Ok(mods.data.into_iter().map(Mod::into_info).collect())
    }
}

/// Find the category whose name contains `keyword`, preferring top-level categories
/// (direct children of the Mods class) over sub-categories.
fn match_category(categories: &[CfCategory], keyword: &str) -> Option<u64> {
    let matching = |c: &&CfCategory| {
        c.class_id == Some(u64::from(CLASS_MODS)) && c.name.to_lowercase().contains(keyword)
    };
    categories
        .iter()
        .filter(matching)
        .find(|c| c.parent_category_id == Some(u64::from(CLASS_MODS)))
        .or_else(|| categories.iter().find(matching))
        .map(|c| c.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    // CurseForge can't be called without a key, so these run against the example
    // responses from its API documentation and hand-written cases.

    #[test]
    fn parses_documented_search_response() {
        let response: Envelope<Vec<Mod>> = serde_json::from_str(include_str!(
            "../../tests/fixtures/curseforge-search-docs-example.json"
        ))
        .unwrap();
        assert!(response.pagination.is_some());
        let hit = response.data.into_iter().next().unwrap().into_hit();
        assert_eq!(hit.source, Source::Curseforge);
        assert!(hit.downloadable);
    }

    #[test]
    fn parses_documented_file_response() {
        let response: Envelope<File> = serde_json::from_str(include_str!(
            "../../tests/fixtures/curseforge-file-docs-example.json"
        ))
        .unwrap();
        let file = response.data.into_file();
        assert_eq!(file.release_type, ReleaseType::Release);
        assert!(file.sha1.is_some());
    }

    #[test]
    fn distribution_flag_and_missing_download_url() {
        let blocked: Mod = serde_json::from_str(
            r#"{"id":1,"name":"Blocked","slug":"blocked","allowModDistribution":false}"#,
        )
        .unwrap();
        let hit = blocked.into_hit();
        assert!(!hit.downloadable);
        assert_eq!(
            hit.page_url,
            "https://www.curseforge.com/minecraft/mc-mods/blocked"
        );

        // Null means "not stated", which CurseForge treats as allowed.
        let unstated: Mod = serde_json::from_str(
            r#"{"id":2,"name":"Fine","slug":"fine","allowModDistribution":null}"#,
        )
        .unwrap();
        assert!(unstated.into_hit().downloadable);

        let file: File = serde_json::from_str(
            r#"{"id":10,"modId":1,"fileName":"blocked-1.0.jar","releaseType":2,"downloadUrl":null,
                "hashes":[{"value":"md5md5","algo":2},{"value":"sha1sha1","algo":1}],
                "dependencies":[{"modId":5,"relationType":3},{"modId":6,"relationType":2}]}"#,
        )
        .unwrap();
        let file = file.into_file();
        assert_eq!(file.url, None);
        assert_eq!(file.sha1.as_deref(), Some("sha1sha1"));
        assert_eq!(file.release_type, ReleaseType::Beta);
        assert_eq!(file.version_name, "blocked-1.0.jar");
        assert_eq!(
            file.dependencies,
            [Dependency {
                project_id: "5".into(),
                version_id: None
            }]
        );
    }

    #[test]
    fn category_matching_prefers_top_level() {
        let categories: Vec<CfCategory> = serde_json::from_str(
            r#"[{"id":1,"name":"Technology","classId":6,"parentCategoryId":6},
                {"id":2,"name":"Energy Technology","classId":6,"parentCategoryId":1},
                {"id":3,"name":"Technology","classId":4471,"parentCategoryId":4471},
                {"id":4,"name":"Performance","classId":6,"parentCategoryId":6}]"#,
        )
        .unwrap();
        assert_eq!(match_category(&categories, "technology"), Some(1));
        assert_eq!(match_category(&categories, "performance"), Some(4));
        assert_eq!(match_category(&categories, "energy"), Some(2));
        assert_eq!(match_category(&categories, "magic"), None);
    }
}

//! Mod discovery and management: one model over Modrinth and CurseForge.

pub mod curseforge;
pub mod index;
pub mod manage;
pub mod modrinth;

use serde::{Deserialize, Serialize};

/// Glint only creates Fabric instances.
pub const LOADER: &str = "fabric";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Modrinth,
    Curseforge,
}

/// Ordered best first, so `min` picks the most stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseType {
    Release,
    Beta,
    Alpha,
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    pub mc_version: Option<String>,
    /// Restrict to mods for [`LOADER`].
    pub fabric_only: bool,
    /// One of [`CATEGORIES`]' ids.
    pub category: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchHit {
    pub source: Source,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub icon_url: Option<String>,
    pub downloads: u64,
    pub page_url: String,
    /// False when the author has turned off third-party downloads (CurseForge only).
    /// Such a mod can only be fetched from its page in a browser.
    pub downloadable: bool,
}

#[derive(Debug, Clone, Default)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
}

/// What an installer needs to know about a project.
#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub icon_url: Option<String>,
    pub page_url: String,
    /// Loaders the project supports at all, when the site says. Empty if unknown.
    pub loaders: Vec<String>,
}

/// One downloadable build of a mod.
#[derive(Debug, Clone)]
pub struct ModFile {
    pub source: Source,
    pub project_id: String,
    pub version_id: String,
    pub version_name: String,
    pub filename: String,
    /// `None` when the site won't hand out a download link for this file.
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: u64,
    pub release_type: ReleaseType,
    /// ISO 8601 timestamp; compares correctly as text within one source.
    pub published: String,
    pub game_versions: Vec<String>,
    /// Required dependencies only.
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub project_id: String,
    /// Set when the mod asks for one exact build of the dependency.
    pub version_id: Option<String>,
}

/// The build to install: the newest release if there is one, otherwise the newest
/// beta, otherwise the newest alpha. Stability wins over freshness, so a mod with a
/// release from September and a beta from last week installs the release.
pub fn pick_file(files: &[ModFile]) -> Option<&ModFile> {
    let best = files.iter().map(|f| f.release_type).min()?;
    files
        .iter()
        .filter(|f| f.release_type == best)
        .max_by(|a, b| a.published.cmp(&b.published))
}

/// A category that exists, under different names, on both sites.
#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: &'static str,
    pub label: &'static str,
    /// Modrinth's category name.
    #[serde(skip)]
    pub modrinth: &'static str,
    /// A word that identifies the matching CurseForge category by name. `None` if
    /// CurseForge has no equivalent, in which case it is left out of filtered searches.
    #[serde(skip)]
    pub curseforge: Option<&'static str>,
}

pub const CATEGORIES: &[Category] = &[
    category(
        "optimization",
        "Optimization",
        "optimization",
        Some("performance"),
    ),
    category("utility", "Utility", "utility", Some("utility")),
    category("library", "Library", "library", Some("library")),
    category("adventure", "Adventure", "adventure", Some("adventure")),
    category("decoration", "Decoration", "decoration", Some("cosmetic")),
    category("equipment", "Equipment", "equipment", Some("armor")),
    category("food", "Food", "food", Some("food")),
    category("game-mechanics", "Game mechanics", "game-mechanics", None),
    category("magic", "Magic", "magic", Some("magic")),
    category("mobs", "Mobs", "mobs", Some("mobs")),
    category("social", "Social", "social", None),
    category("storage", "Storage", "storage", Some("storage")),
    category("technology", "Technology", "technology", Some("technology")),
    category("transportation", "Transportation", "transportation", None),
    category(
        "worldgen",
        "World generation",
        "worldgen",
        Some("world gen"),
    ),
];

const fn category(
    id: &'static str,
    label: &'static str,
    modrinth: &'static str,
    curseforge: Option<&'static str>,
) -> Category {
    Category {
        id,
        label,
        modrinth,
        curseforge,
    }
}

pub fn find_category(id: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(id: &str, release_type: ReleaseType, published: &str) -> ModFile {
        ModFile {
            source: Source::Modrinth,
            project_id: "p".into(),
            version_id: id.into(),
            version_name: id.into(),
            filename: format!("{id}.jar"),
            url: None,
            sha1: None,
            size: 0,
            release_type,
            published: published.into(),
            game_versions: vec![],
            dependencies: vec![],
        }
    }

    #[test]
    fn prefers_newest_release_over_newer_prereleases() {
        let files = [
            file("beta-new", ReleaseType::Beta, "2026-11-08"),
            file("alpha", ReleaseType::Alpha, "2026-09-20"),
            file("release-old", ReleaseType::Release, "2026-09-01"),
            file("release-new", ReleaseType::Release, "2026-09-15"),
        ];
        assert_eq!(pick_file(&files).unwrap().version_id, "release-new");
    }

    #[test]
    fn falls_back_to_prereleases() {
        let files = [
            file("alpha", ReleaseType::Alpha, "2026-11-01"),
            file("beta-old", ReleaseType::Beta, "2026-10-01"),
            file("beta-new", ReleaseType::Beta, "2026-10-15"),
        ];
        assert_eq!(pick_file(&files).unwrap().version_id, "beta-new");
        assert!(pick_file(&[]).is_none());
    }
}

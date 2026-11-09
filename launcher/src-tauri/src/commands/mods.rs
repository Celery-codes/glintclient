use std::collections::HashSet;

use glint_core::{
    Error,
    config::Config,
    instance::{self, Instance},
    mods::{
        CATEGORIES, Category, SearchHit, SearchPage, SearchQuery, Source,
        curseforge::CurseForge,
        index::{self, InstalledMod},
        manage::{self, Context, InstallOutcome, ModUpdate},
        modrinth,
    },
};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::{CmdResult, ensure_idle};
use crate::state::AppState;

const PAGE_SIZE: u32 = 20;

#[derive(Deserialize)]
pub struct SearchRequest {
    text: String,
    /// `all`, `modrinth` or `curseforge`.
    source: String,
    category: Option<String>,
    mc_version: Option<String>,
    fabric_only: bool,
    page: u32,
}

#[derive(Serialize)]
pub struct SearchResponse {
    hits: Vec<SearchHit>,
    has_more: bool,
    /// False when no CurseForge API key is configured.
    curseforge_enabled: bool,
    /// Problems with one site that didn't stop the other from answering.
    warnings: Vec<String>,
}

#[tauri::command]
pub fn mod_categories() -> Vec<Category> {
    CATEGORIES.to_vec()
}

/// Alternate the two result lists, dropping CurseForge entries for mods Modrinth
/// already returned. Modrinth is preferred because its downloads are never blocked.
fn merge(modrinth: Vec<SearchHit>, curseforge: Vec<SearchHit>) -> Vec<SearchHit> {
    let on_modrinth: HashSet<String> = modrinth.iter().map(|h| h.slug.to_lowercase()).collect();
    let mut curseforge = curseforge
        .into_iter()
        .filter(|h| !on_modrinth.contains(&h.slug.to_lowercase()));
    let mut merged = Vec::new();
    for hit in modrinth {
        merged.push(hit);
        merged.extend(curseforge.next());
    }
    merged.extend(curseforge);
    merged
}

#[tauri::command]
pub async fn search_mods(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> CmdResult<SearchResponse> {
    let config = Config::load(&state.paths).await?;
    let api_key = config.curseforge_api_key().map(|key| key.value);
    let query = SearchQuery {
        text: request.text.trim().to_string(),
        mc_version: request.mc_version.filter(|v| !v.is_empty()),
        fabric_only: request.fabric_only,
        category: request.category.filter(|c| !c.is_empty()),
        page: request.page,
        page_size: PAGE_SIZE,
    };
    let want_modrinth = request.source != "curseforge";
    let want_curseforge = request.source != "modrinth";
    if !want_modrinth && api_key.is_none() {
        return Err(Error::Config(
            "Add a CurseForge API key in Settings to search CurseForge.".into(),
        ));
    }

    let modrinth_search = async {
        if want_modrinth {
            Some(modrinth::search(&state.client, &query).await)
        } else {
            None
        }
    };
    let curseforge_search = async {
        match (&api_key, want_curseforge) {
            (Some(api_key), true) => {
                let curseforge = CurseForge {
                    client: &state.client,
                    api_key,
                };
                // `None` inside: CurseForge has no such category, so it sits this one out.
                curseforge.search(&query).await.transpose()
            }
            _ => None,
        }
    };
    let (modrinth_result, curseforge_result) = tokio::join!(modrinth_search, curseforge_search);

    let mut warnings = Vec::new();
    let mut failures = Vec::new();
    let mut unpack = |name: &str, result: Option<Result<SearchPage, Error>>| match result {
        Some(Ok(page)) => Some(page),
        Some(Err(error)) => {
            warnings.push(format!("{name} search failed: {error}"));
            failures.push(error);
            None
        }
        None => None,
    };
    let modrinth_page = unpack("Modrinth", modrinth_result);
    let curseforge_page = unpack("CurseForge", curseforge_result);
    // If nothing answered, that's an error, not an empty result.
    if modrinth_page.is_none()
        && curseforge_page.is_none()
        && let Some(error) = failures.into_iter().next()
    {
        return Err(error);
    }

    let shown = u64::from((request.page + 1) * PAGE_SIZE);
    let has_more = [&modrinth_page, &curseforge_page]
        .iter()
        .any(|page| page.as_ref().is_some_and(|p| p.total > shown));
    Ok(SearchResponse {
        hits: merge(
            modrinth_page.map(|p| p.hits).unwrap_or_default(),
            curseforge_page.map(|p| p.hits).unwrap_or_default(),
        ),
        has_more,
        curseforge_enabled: api_key.is_some(),
        warnings,
    })
}

/// The instance and settings a mod operation needs, loaded once per command.
struct Loaded {
    instance: Instance,
    curseforge_key: Option<String>,
}

impl Loaded {
    async fn new(state: &AppState, id: &str) -> CmdResult<Self> {
        Ok(Self {
            instance: instance::get(&state.paths, id).await?,
            curseforge_key: Config::load(&state.paths)
                .await?
                .curseforge_api_key()
                .map(|key| key.value),
        })
    }

    fn context<'a>(&'a self, state: &'a AppState) -> Context<'a> {
        Context {
            client: &state.client,
            paths: &state.paths,
            instance: &self.instance,
            curseforge_key: self.curseforge_key.as_deref(),
        }
    }
}

#[tauri::command]
pub async fn list_instance_mods(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<InstalledMod>> {
    let instance = instance::get(&state.paths, &id).await?;
    let _guard = state.mods_lock.lock().await;
    index::scan(&state.paths, &instance.id).await
}

#[tauri::command]
pub async fn install_mod(
    state: State<'_, AppState>,
    id: String,
    source: Source,
    project: String,
) -> CmdResult<InstallOutcome> {
    let loaded = Loaded::new(&state, &id).await?;
    let _guard = state.mods_lock.lock().await;
    loaded.context(&state).install(source, &project).await
}

/// Install the mods the instance's preset lists that aren't there yet.
#[tauri::command]
pub async fn install_preset_mods(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<InstallOutcome> {
    let loaded = Loaded::new(&state, &id).await?;
    let _guard = state.mods_lock.lock().await;
    loaded
        .context(&state)
        .install_many(&loaded.instance.mods)
        .await
}

#[tauri::command]
pub async fn set_mod_enabled(
    state: State<'_, AppState>,
    id: String,
    filename: String,
    enabled: bool,
) -> CmdResult<InstalledMod> {
    ensure_idle(&state, &id, "change the mods of")?;
    let instance = instance::get(&state.paths, &id).await?;
    let _guard = state.mods_lock.lock().await;
    manage::set_enabled(&state.paths, &instance.id, &filename, enabled).await
}

#[tauri::command]
pub async fn remove_mod(state: State<'_, AppState>, id: String, filename: String) -> CmdResult<()> {
    ensure_idle(&state, &id, "change the mods of")?;
    let instance = instance::get(&state.paths, &id).await?;
    let _guard = state.mods_lock.lock().await;
    manage::remove(&state.paths, &instance.id, &filename).await
}

#[tauri::command]
pub async fn check_mod_updates(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<ModUpdate>> {
    let loaded = Loaded::new(&state, &id).await?;
    let _guard = state.mods_lock.lock().await;
    loaded.context(&state).check_updates().await
}

#[tauri::command]
pub async fn update_mod(
    state: State<'_, AppState>,
    id: String,
    filename: String,
) -> CmdResult<InstalledMod> {
    ensure_idle(&state, &id, "change the mods of")?;
    let loaded = Loaded::new(&state, &id).await?;
    let _guard = state.mods_lock.lock().await;
    loaded.context(&state).update(&filename).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(source: Source, slug: &str) -> SearchHit {
        SearchHit {
            source,
            project_id: slug.into(),
            slug: slug.into(),
            title: slug.into(),
            description: String::new(),
            author: String::new(),
            icon_url: None,
            downloads: 0,
            page_url: String::new(),
            downloadable: true,
        }
    }

    #[test]
    fn merge_interleaves_and_prefers_modrinth() {
        let modrinth = vec![
            hit(Source::Modrinth, "sodium"),
            hit(Source::Modrinth, "lithium"),
        ];
        let curseforge = vec![
            hit(Source::Curseforge, "Sodium"),
            hit(Source::Curseforge, "jei"),
            hit(Source::Curseforge, "waila"),
            hit(Source::Curseforge, "extra"),
        ];
        let merged: Vec<_> = merge(modrinth, curseforge)
            .into_iter()
            .map(|h| (h.source, h.slug))
            .collect();
        assert_eq!(
            merged,
            [
                (Source::Modrinth, "sodium".to_string()),
                (Source::Curseforge, "jei".to_string()),
                (Source::Modrinth, "lithium".to_string()),
                (Source::Curseforge, "waila".to_string()),
                (Source::Curseforge, "extra".to_string()),
            ]
        );
    }
}

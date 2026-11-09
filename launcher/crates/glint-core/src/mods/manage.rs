//! Installing, updating, enabling and removing mods in an instance.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::Serialize;

use super::{
    LOADER, ModFile, ProjectInfo, Source,
    curseforge::CurseForge,
    index::{self, DISABLED_SUFFIX, InstalledMod},
    modrinth, pick_file,
};
use crate::{
    Error, Result,
    download::{self, Download, Hash},
    instance::{Instance, ModRef},
    paths::Paths,
};

const LOOKUP_CONCURRENCY: usize = 8;

pub struct Context<'a> {
    pub client: &'a reqwest::Client,
    pub paths: &'a Paths,
    pub instance: &'a Instance,
    pub curseforge_key: Option<&'a str>,
}

/// A mod that has to be downloaded by hand because its site won't give Glint a link.
#[derive(Debug, Clone, Serialize)]
pub struct ManualDownload {
    pub title: String,
    pub page_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub project: String,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct InstallOutcome {
    pub installed: Vec<InstalledMod>,
    /// Titles of requested mods that were already there.
    pub already_installed: Vec<String>,
    pub manual: Vec<ManualDownload>,
    pub failed: Vec<Failure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModUpdate {
    pub filename: String,
    pub title: String,
    pub current_version: String,
    pub new_version: String,
}

struct Request {
    source: Source,
    /// Project id, or slug for a root request.
    project: String,
    pinned_version: Option<String>,
    is_root: bool,
}

impl Context<'_> {
    fn curseforge(&self) -> Result<CurseForge<'_>> {
        let api_key = self.curseforge_key.ok_or_else(|| {
            Error::Config("Add a CurseForge API key in Settings to use CurseForge mods.".into())
        })?;
        Ok(CurseForge {
            client: self.client,
            api_key,
        })
    }

    async fn files(&self, source: Source, project: &str) -> Result<Vec<ModFile>> {
        let mc_version = &self.instance.mc_version;
        match source {
            Source::Modrinth => modrinth::files(self.client, project, mc_version).await,
            Source::Curseforge => self.curseforge()?.files(project, mc_version).await,
        }
    }

    async fn projects(&self, source: Source, ids: &[String]) -> Result<Vec<ProjectInfo>> {
        match source {
            Source::Modrinth => modrinth::projects(self.client, ids).await,
            Source::Curseforge => self.curseforge()?.projects(ids).await,
        }
    }

    /// The build of `project` to use on this instance.
    /// `Ok(None)` means the project exists but has nothing for this Minecraft version
    /// on Fabric.
    async fn choose(
        &self,
        source: Source,
        project: &str,
        pinned_version: Option<&str>,
    ) -> Result<Option<ModFile>> {
        let files = self.files(source, project).await?;
        // A dependency may ask for one exact build. Honour that only if the build is
        // for this Minecraft version; otherwise the normal choice is the better guess.
        let pinned = pinned_version.and_then(|id| files.iter().find(|f| f.version_id == id));
        Ok(pinned.or_else(|| pick_file(&files)).cloned())
    }

    /// Record that `request` has no usable build, unless it doesn't matter.
    ///
    /// Mods that ship one jar for several loaders list every loader's dependencies as
    /// required. A dependency whose project doesn't support Fabric at all (a NeoForge
    /// language adapter, say) isn't needed here, so it is skipped without comment.
    async fn report_no_build(&self, request: Request, outcome: &mut InstallOutcome) {
        let info = self
            .projects(request.source, std::slice::from_ref(&request.project))
            .await
            .ok()
            .and_then(|projects| projects.into_iter().next());
        if let Some(info) = &info
            && !request.is_root
            && !info.loaders.is_empty()
            && !info.loaders.iter().any(|loader| loader == LOADER)
        {
            return;
        }
        let version = &self.instance.mc_version;
        outcome.failed.push(Failure {
            project: info.map_or(request.project, |info| info.title),
            message: if request.is_root {
                format!("It has no build for Minecraft {version} on Fabric.")
            } else {
                format!(
                    "It has no build for Minecraft {version} on Fabric, and a mod you installed requires it, so that mod may not work."
                )
            },
        });
    }

    /// Work out every file needed for `requests`, following required dependencies.
    /// Problems with individual projects are recorded in `outcome`, not returned.
    async fn resolve(
        &self,
        requests: Vec<Request>,
        installed: &[InstalledMod],
        outcome: &mut InstallOutcome,
    ) -> Vec<ModFile> {
        let mut queue: VecDeque<Request> = requests.into();
        let mut seen: HashSet<(Source, String)> = HashSet::new();
        let mut plan = Vec::new();

        while let Some(request) = queue.pop_front() {
            let existing = |project: &str| {
                installed
                    .iter()
                    .find(|m| m.is_project(request.source, project))
            };
            if let Some(found) = existing(&request.project) {
                if request.is_root {
                    outcome.already_installed.push(found.title.clone());
                }
                continue;
            }
            let file = match self
                .choose(
                    request.source,
                    &request.project,
                    request.pinned_version.as_deref(),
                )
                .await
            {
                Ok(Some(file)) => file,
                Ok(None) => {
                    self.report_no_build(request, outcome).await;
                    continue;
                }
                Err(error) => {
                    outcome.failed.push(Failure {
                        project: request.project,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            // A slug request only reveals its project id here, so check again.
            if let Some(found) = existing(&file.project_id) {
                if request.is_root {
                    outcome.already_installed.push(found.title.clone());
                }
                continue;
            }
            if !seen.insert((file.source, file.project_id.clone())) {
                continue;
            }
            for dependency in &file.dependencies {
                queue.push_back(Request {
                    source: file.source,
                    project: dependency.project_id.clone(),
                    pinned_version: dependency.version_id.clone(),
                    is_root: false,
                });
            }
            plan.push(file);
        }
        plan
    }

    /// Titles, icons and page links for the planned files. Best effort: a failed
    /// lookup just means the mod is listed by file name.
    async fn describe(&self, plan: &[ModFile]) -> HashMap<(Source, String), ProjectInfo> {
        let mut info = HashMap::new();
        for source in [Source::Modrinth, Source::Curseforge] {
            let ids: Vec<String> = plan
                .iter()
                .filter(|f| f.source == source)
                .map(|f| f.project_id.clone())
                .collect();
            if ids.is_empty() {
                continue;
            }
            if let Ok(projects) = self.projects(source, &ids).await {
                for project in projects {
                    info.insert((source, project.project_id.clone()), project);
                }
            }
        }
        info
    }

    async fn install_requests(&self, requests: Vec<Request>) -> Result<InstallOutcome> {
        let id = &self.instance.id;
        let mut mods = index::scan(self.paths, id).await?;
        let mut outcome = InstallOutcome::default();
        let plan = self.resolve(requests, &mods, &mut outcome).await;
        if plan.is_empty() {
            return Ok(outcome);
        }
        let info = self.describe(&plan).await;
        let mods_dir = index::mods_dir(self.paths, id);
        tokio::fs::create_dir_all(&mods_dir).await?;

        // Decide what can be downloaded, then fetch those in parallel.
        let mut downloads = Vec::new();
        for file in plan {
            let project = info.get(&(file.source, file.project_id.clone()));
            let title = project.map_or_else(|| file.filename.clone(), |p| p.title.clone());
            let Some(url) = file.url.clone() else {
                outcome.manual.push(ManualDownload {
                    title,
                    page_url: project.map_or_else(
                        || fallback_page_url(file.source, &file.project_id),
                        |p| p.page_url.clone(),
                    ),
                });
                continue;
            };
            let filename = match index::safe_filename(&file.filename) {
                Ok(filename) => filename,
                Err(error) => {
                    outcome.failed.push(Failure {
                        project: title,
                        message: error.to_string(),
                    });
                    continue;
                }
            };
            let download = Download {
                url,
                path: mods_dir.join(&filename),
                hash: file.sha1.clone().map(Hash::Sha1),
                size: Some(file.size).filter(|size| *size > 0),
            };
            let installed = InstalledMod {
                filename,
                enabled: true,
                title,
                version_name: file.version_name,
                source: Some(file.source),
                project_id: Some(file.project_id),
                slug: project.map(|p| p.slug.clone()),
                version_id: Some(file.version_id),
                icon_url: project.and_then(|p| p.icon_url.clone()),
                page_url: project.map(|p| p.page_url.clone()),
                sha1: file.sha1,
                published: Some(file.published),
                size: file.size,
            };
            downloads.push((download, installed));
        }

        let client = self.client;
        let results = futures::future::join_all(downloads.into_iter().map(
            |(download, installed)| async move {
                let result = download::fetch(client, &download).await;
                (result, installed)
            },
        ))
        .await;

        for (result, installed) in results {
            match result {
                Ok(()) => {
                    // Installing a mod that was sitting there disabled replaces it.
                    let disabled =
                        mods_dir.join(format!("{}{DISABLED_SUFFIX}", installed.filename));
                    let _ = tokio::fs::remove_file(disabled).await;
                    mods.retain(|m| m.filename != installed.filename);
                    mods.push(installed.clone());
                    outcome.installed.push(installed);
                }
                Err(error) => outcome.failed.push(Failure {
                    project: installed.title,
                    message: error.to_string(),
                }),
            }
        }
        index::save(self.paths, id, &mods).await?;
        Ok(outcome)
    }

    /// Install one mod and whatever it requires.
    pub async fn install(&self, source: Source, project: &str) -> Result<InstallOutcome> {
        self.install_requests(vec![Request {
            source,
            project: project.to_string(),
            pinned_version: None,
            is_root: true,
        }])
        .await
    }

    /// Install a list of mods (a preset's), skipping any that are already present.
    pub async fn install_many(&self, mods: &[ModRef]) -> Result<InstallOutcome> {
        let mut outcome_failures = Vec::new();
        let mut requests = Vec::new();
        for reference in mods {
            let source = match reference.source.as_str() {
                "modrinth" => Source::Modrinth,
                "curseforge" => Source::Curseforge,
                other => {
                    outcome_failures.push(Failure {
                        project: reference.project.clone(),
                        message: format!("unknown mod source {other}"),
                    });
                    continue;
                }
            };
            requests.push(Request {
                source,
                project: reference.project.clone(),
                pinned_version: None,
                is_root: true,
            });
        }
        let mut outcome = self.install_requests(requests).await?;
        outcome.failed.extend(outcome_failures);
        Ok(outcome)
    }

    /// The newer build available for an installed mod, if any.
    async fn newer_build(&self, installed: &InstalledMod) -> Option<ModFile> {
        let (source, project) = (installed.source?, installed.project_id.as_deref()?);
        // No CurseForge key: those mods just aren't checked.
        let file = self.choose(source, project, None).await.ok()??;
        let different = installed.version_id.as_deref() != Some(file.version_id.as_str());
        // Someone on a beta newer than the latest release shouldn't be offered the
        // older release as an "update".
        let newer = installed
            .published
            .as_deref()
            .is_none_or(|published| file.published.as_str() > published);
        (different && newer).then_some(file)
    }

    /// Recognise hand-copied jars, then report which mods have a newer build.
    pub async fn check_updates(&self) -> Result<Vec<ModUpdate>> {
        let id = &self.instance.id;
        let mut mods = index::scan(self.paths, id).await?;
        if self.adopt_untracked(&mut mods).await.unwrap_or(false) {
            index::save(self.paths, id, &mods).await?;
        }

        let mut updates = Vec::new();
        for chunk in mods.chunks(LOOKUP_CONCURRENCY) {
            let lookups = chunk.iter().cloned().map(|installed| async move {
                let newer = self.newer_build(&installed).await;
                (installed, newer)
            });
            for (installed, newer) in futures::future::join_all(lookups).await {
                if let Some(file) = newer {
                    updates.push(ModUpdate {
                        filename: installed.filename,
                        title: installed.title,
                        current_version: installed.version_name,
                        new_version: file.version_name,
                    });
                }
            }
        }
        Ok(updates)
    }

    /// Look up untracked jars on Modrinth by hash and fill in what they are. Returns
    /// whether anything changed.
    async fn adopt_untracked(&self, mods: &mut [InstalledMod]) -> Result<bool> {
        let hashes: Vec<String> = mods
            .iter()
            .filter(|m| m.source.is_none())
            .filter_map(|m| m.sha1.clone())
            .collect();
        let found = modrinth::identify(self.client, &hashes).await?;
        if found.is_empty() {
            return Ok(false);
        }
        let files: Vec<ModFile> = found.values().cloned().collect();
        let info = self.describe(&files).await;
        for installed in mods.iter_mut().filter(|m| m.source.is_none()) {
            let Some(file) = installed.sha1.as_ref().and_then(|sha1| found.get(sha1)) else {
                continue;
            };
            let project = info.get(&(file.source, file.project_id.clone()));
            installed.source = Some(file.source);
            installed.project_id = Some(file.project_id.clone());
            installed.version_id = Some(file.version_id.clone());
            installed.version_name = file.version_name.clone();
            installed.published = Some(file.published.clone());
            if let Some(project) = project {
                installed.title = project.title.clone();
                installed.slug = Some(project.slug.clone());
                installed.icon_url = project.icon_url.clone();
                installed.page_url = Some(project.page_url.clone());
            }
        }
        Ok(true)
    }

    /// Replace an installed mod with its newest build, keeping it enabled or disabled
    /// as it was, and install anything the new build newly requires.
    pub async fn update(&self, filename: &str) -> Result<InstalledMod> {
        let id = &self.instance.id;
        let mut mods = index::scan(self.paths, id).await?;
        let position = mods
            .iter()
            .position(|m| m.filename == filename)
            .ok_or_else(|| Error::NotFound(format!("mod {filename}")))?;
        let current = mods[position].clone();
        let file = self
            .newer_build(&current)
            .await
            .ok_or_else(|| Error::Other(format!("{} is already up to date.", current.title)))?;
        let url = file.url.clone().ok_or_else(|| {
            Error::Other(format!(
                "{} can only be downloaded from its page: {}",
                current.title,
                current.page_url.clone().unwrap_or_default()
            ))
        })?;

        let new_filename = index::safe_filename(&file.filename)?;
        let mods_dir = index::mods_dir(self.paths, id);
        let download = Download {
            url,
            path: mods_dir.join(&new_filename),
            hash: file.sha1.clone().map(Hash::Sha1),
            size: Some(file.size).filter(|size| *size > 0),
        };
        // Download first: if it fails, the old build is still in place.
        download::fetch(self.client, &download).await?;
        if new_filename != current.filename || !current.enabled {
            remove_file(&index::file_path(self.paths, id, &current)).await?;
        }
        if !current.enabled {
            let disabled = mods_dir.join(format!("{new_filename}{DISABLED_SUFFIX}"));
            tokio::fs::rename(&download.path, &disabled).await?;
        }

        let updated = InstalledMod {
            filename: new_filename,
            version_name: file.version_name.clone(),
            version_id: Some(file.version_id.clone()),
            sha1: file.sha1.clone(),
            published: Some(file.published.clone()),
            size: file.size,
            ..current
        };
        mods[position] = updated.clone();
        index::save(self.paths, id, &mods).await?;

        let dependencies = file
            .dependencies
            .iter()
            .map(|d| Request {
                source: file.source,
                project: d.project_id.clone(),
                pinned_version: d.version_id.clone(),
                is_root: false,
            })
            .collect::<Vec<_>>();
        if !dependencies.is_empty() {
            self.install_requests(dependencies).await?;
        }
        Ok(updated)
    }
}

fn fallback_page_url(source: Source, project_id: &str) -> String {
    match source {
        Source::Modrinth => format!("https://modrinth.com/mod/{project_id}"),
        Source::Curseforge => format!("https://www.curseforge.com/projects/{project_id}"),
    }
}

/// Deleting or renaming a jar the running game has open fails on Windows; say why.
fn explain_locked(error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(32) {
        Error::Other("That mod's file is in use. Stop the game and try again.".into())
    } else {
        error.into()
    }
}

async fn remove_file(path: &std::path::Path) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(explain_locked(e)),
    }
}

async fn find(
    paths: &Paths,
    instance_id: &str,
    filename: &str,
) -> Result<(Vec<InstalledMod>, usize)> {
    let mods = index::scan(paths, instance_id).await?;
    let position = mods
        .iter()
        .position(|m| m.filename == filename)
        .ok_or_else(|| Error::NotFound(format!("mod {filename}")))?;
    Ok((mods, position))
}

pub async fn set_enabled(
    paths: &Paths,
    instance_id: &str,
    filename: &str,
    enabled: bool,
) -> Result<InstalledMod> {
    let (mut mods, position) = find(paths, instance_id, filename).await?;
    if mods[position].enabled != enabled {
        let from = index::file_path(paths, instance_id, &mods[position]);
        mods[position].enabled = enabled;
        let to = index::file_path(paths, instance_id, &mods[position]);
        tokio::fs::rename(from, to).await.map_err(explain_locked)?;
        index::save(paths, instance_id, &mods).await?;
    }
    Ok(mods[position].clone())
}

pub async fn remove(paths: &Paths, instance_id: &str, filename: &str) -> Result<()> {
    let (mut mods, position) = find(paths, instance_id, filename).await?;
    remove_file(&index::file_path(paths, instance_id, &mods[position])).await?;
    mods.remove(position);
    index::save(paths, instance_id, &mods).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_mod(paths: &Paths, name: &str) {
        let dir = index::mods_dir(paths, "inst");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), name.as_bytes()).unwrap();
    }

    #[tokio::test]
    async fn disabling_renames_and_enabling_restores() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        write_mod(&paths, "a.jar");
        let mods_dir = index::mods_dir(&paths, "inst");

        let off = set_enabled(&paths, "inst", "a.jar", false).await.unwrap();
        assert!(!off.enabled);
        assert!(mods_dir.join("a.jar.disabled").is_file());
        assert!(!mods_dir.join("a.jar").exists());
        assert!(!index::scan(&paths, "inst").await.unwrap()[0].enabled);

        // Asking for the state it's already in is a no-op, not an error.
        set_enabled(&paths, "inst", "a.jar", false).await.unwrap();

        let on = set_enabled(&paths, "inst", "a.jar", true).await.unwrap();
        assert!(on.enabled);
        assert!(mods_dir.join("a.jar").is_file());

        assert!(
            set_enabled(&paths, "inst", "missing.jar", true)
                .await
                .is_err()
        );
        assert!(set_enabled(&paths, "inst", "../a.jar", true).await.is_err());
    }

    #[tokio::test]
    async fn remove_deletes_either_state() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        write_mod(&paths, "a.jar");
        write_mod(&paths, "b.jar.disabled");

        remove(&paths, "inst", "a.jar").await.unwrap();
        remove(&paths, "inst", "b.jar").await.unwrap();
        assert!(index::scan(&paths, "inst").await.unwrap().is_empty());
        assert_eq!(
            std::fs::read_dir(index::mods_dir(&paths, "inst"))
                .unwrap()
                .count(),
            0
        );
        assert!(remove(&paths, "inst", "a.jar").await.is_err());
    }
}

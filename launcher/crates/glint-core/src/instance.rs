use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, fs_util, paths::Paths, system};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GcPreset {
    /// Tuned G1: the long-standing choice for the client, with short pause targets.
    #[default]
    G1,
    /// Generational ZGC. Lower pauses, more CPU and memory overhead. Java 21+.
    Zgc,
}

/// A mod a preset wants, to be installed by the mod browser (Phase 2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRef {
    /// `modrinth` or `curseforge`.
    pub source: String,
    /// Project slug on that source.
    pub project: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub preset: Option<String>,
    pub mc_version: String,
    pub loader_version: String,
    pub memory_mb: u32,
    #[serde(default)]
    pub gc: GcPreset,
    /// Extra JVM arguments, space separated. Applied last, so they win.
    #[serde(default)]
    pub jvm_args: String,
    /// Use this Java executable instead of the managed Temurin runtime.
    #[serde(default)]
    pub java_path: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub mods: Vec<ModRef>,
    pub created_at: u64,
    #[serde(default)]
    pub last_played: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewInstance {
    pub name: String,
    pub preset: Option<String>,
    pub mc_version: String,
    pub loader_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// Preferred allocation; reduced on machines with little RAM.
    pub memory_mb: u32,
    pub gc: GcPreset,
    pub mods: Vec<ModRef>,
}

fn modrinth(projects: &[&str]) -> Vec<ModRef> {
    projects
        .iter()
        .map(|p| ModRef {
            source: "modrinth".into(),
            project: (*p).into(),
        })
        .collect()
}

/// The optimisation mods every preset builds on.
const PERFORMANCE_MODS: &[&str] = &[
    "fabric-api",
    "sodium",
    "lithium",
    "ferrite-core",
    "immediatelyfast",
    "entityculling",
];

pub fn presets() -> Vec<Preset> {
    let with = |extra: &[&str]| modrinth(&[PERFORMANCE_MODS, extra].concat());
    vec![
        Preset {
            id: "pvp",
            name: "PvP",
            description: "Highest, steadiest frame rate for competitive servers.",
            memory_mb: 4096,
            gc: GcPreset::G1,
            mods: with(&[]),
        },
        Preset {
            id: "smp",
            name: "SMP",
            description: "Survival multiplayer with more headroom for big bases and long sessions.",
            memory_mb: 6144,
            gc: GcPreset::G1,
            mods: with(&["modmenu"]),
        },
        Preset {
            id: "performance",
            name: "Performance",
            description: "Just the optimisation mods. A fast, vanilla-feeling game.",
            memory_mb: 4096,
            gc: GcPreset::G1,
            mods: with(&[]),
        },
    ]
}

pub fn preset(id: &str) -> Option<Preset> {
    presets().into_iter().find(|p| p.id == id)
}

/// A filesystem-safe id derived from the name, e.g. `pvp-practice`.
fn slugify(name: &str) -> String {
    let mut slug = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug: String = slug.chars().take(40).collect();
    if slug.is_empty() {
        "instance".into()
    } else {
        slug
    }
}

async fn unique_id(paths: &Paths, name: &str) -> String {
    let base = slugify(name);
    let mut candidate = base.clone();
    let mut n = 2;
    while tokio::fs::try_exists(paths.instance_dir(&candidate))
        .await
        .unwrap_or(false)
    {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    candidate
}

fn validate_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Config("Give the instance a name.".into()));
    }
    Ok(name.to_string())
}

pub async fn create(paths: &Paths, new: NewInstance, total_memory_mb: u32) -> Result<Instance> {
    let name = validate_name(&new.name)?;
    let preset = match new.preset.as_deref() {
        Some(id) => Some(preset(id).ok_or_else(|| Error::NotFound(format!("preset {id}")))?),
        None => None,
    };
    let instance = Instance {
        id: unique_id(paths, &name).await,
        name,
        preset: preset.as_ref().map(|p| p.id.to_string()),
        mc_version: new.mc_version,
        loader_version: new.loader_version,
        memory_mb: system::fit_memory(
            preset.as_ref().map_or(4096, |p| p.memory_mb),
            total_memory_mb,
        ),
        gc: preset.as_ref().map(|p| p.gc).unwrap_or_default(),
        jvm_args: String::new(),
        java_path: None,
        width: None,
        height: None,
        mods: preset.map(|p| p.mods).unwrap_or_default(),
        created_at: fs_util::now_unix(),
        last_played: None,
    };
    tokio::fs::create_dir_all(paths.instance_game_dir(&instance.id)).await?;
    save(paths, &instance).await?;
    Ok(instance)
}

/// Ids become folder names, so anything but `[a-z0-9-]` is rejected outright. This is
/// what stops an id like `..` from pointing outside the instances folder.
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if ok {
        Ok(())
    } else {
        Err(Error::NotFound(format!("instance {id}")))
    }
}

pub async fn save(paths: &Paths, instance: &Instance) -> Result<()> {
    check_id(&instance.id)?;
    validate_name(&instance.name)?;
    fs_util::write_json(&paths.instance_file(&instance.id), instance).await
}

pub async fn get(paths: &Paths, id: &str) -> Result<Instance> {
    check_id(id)?;
    fs_util::read_json_opt(&paths.instance_file(id))
        .await?
        .ok_or_else(|| Error::NotFound(format!("instance {id}")))
}

/// All instances, most recently played first, then newest.
pub async fn list(paths: &Paths) -> Result<Vec<Instance>> {
    let mut instances = Vec::new();
    let mut dir = match tokio::fs::read_dir(paths.instances_dir()).await {
        Ok(dir) => dir,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(instances),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = dir.next_entry().await? {
        // A folder with a missing or unreadable instance.json isn't ours to list.
        if let Ok(Some(instance)) =
            fs_util::read_json_opt::<Instance>(&entry.path().join("instance.json")).await
        {
            instances.push(instance);
        }
    }
    instances.sort_by_key(|i| std::cmp::Reverse((i.last_played, i.created_at)));
    Ok(instances)
}

pub async fn delete(paths: &Paths, id: &str) -> Result<()> {
    get(paths, id).await?;
    tokio::fs::remove_dir_all(paths.instance_dir(id)).await?;
    Ok(())
}

/// Copy an instance with its whole game directory (worlds, mods, settings).
pub async fn duplicate(paths: &Paths, id: &str) -> Result<Instance> {
    let source = get(paths, id).await?;
    let name = format!("{} (copy)", source.name);
    let copy = Instance {
        id: unique_id(paths, &name).await,
        name,
        created_at: fs_util::now_unix(),
        last_played: None,
        ..source.clone()
    };
    let (from, to) = (
        paths.instance_game_dir(&source.id),
        paths.instance_game_dir(&copy.id),
    );
    tokio::task::spawn_blocking(move || copy_dir(&from, &to))
        .await
        .map_err(|e| Error::Other(e.to_string()))??;
    save(paths, &copy).await?;
    Ok(copy)
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    if !from.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub async fn mark_played(paths: &Paths, id: &str) -> Result<()> {
    let mut instance = get(paths, id).await?;
    instance.last_played = Some(fs_util::now_unix());
    save(paths, &instance).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(name: &str, preset: Option<&str>) -> NewInstance {
        NewInstance {
            name: name.into(),
            preset: preset.map(str::to_string),
            mc_version: "26.3".into(),
            loader_version: "0.20.1".into(),
        }
    }

    #[test]
    fn slugs() {
        assert_eq!(slugify("PvP Practice!"), "pvp-practice");
        assert_eq!(slugify("  ../..  "), "instance");
        assert_eq!(slugify("SMP — Season 3"), "smp-season-3");
    }

    #[tokio::test]
    async fn create_applies_preset_and_ids_are_unique() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());

        let a = create(&paths, new("My SMP", Some("smp")), 32768)
            .await
            .unwrap();
        assert_eq!(a.id, "my-smp");
        assert_eq!(a.memory_mb, 6144);
        assert!(a.mods.iter().any(|m| m.project == "sodium"));
        assert!(paths.instance_game_dir(&a.id).is_dir());

        // Same name again gets its own folder; 8 GB machine gets a smaller heap.
        let b = create(&paths, new("My SMP", Some("smp")), 8192)
            .await
            .unwrap();
        assert_eq!(b.id, "my-smp-2");
        assert_eq!(b.memory_mb, 4096);

        assert_eq!(list(&paths).await.unwrap().len(), 2);
        assert!(create(&paths, new("  ", None), 8192).await.is_err());
        assert!(create(&paths, new("x", Some("nope")), 8192).await.is_err());
    }

    #[tokio::test]
    async fn duplicate_copies_game_files_and_delete_removes() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let a = create(&paths, new("Base", None), 16384).await.unwrap();
        let mods = paths.instance_game_dir(&a.id).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), b"jar").unwrap();

        let copy = duplicate(&paths, &a.id).await.unwrap();
        assert_eq!(copy.name, "Base (copy)");
        assert!(
            paths
                .instance_game_dir(&copy.id)
                .join("mods/a.jar")
                .is_file()
        );

        delete(&paths, &a.id).await.unwrap();
        assert!(!paths.instance_dir(&a.id).exists());
        assert!(get(&paths, &a.id).await.is_err());
        // Path traversal can't reach outside the instances folder.
        assert!(delete(&paths, "..").await.is_err());
        assert!(dir.path().exists());
    }

    #[tokio::test]
    async fn list_orders_by_last_played() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path());
        let a = create(&paths, new("A", None), 16384).await.unwrap();
        let _b = create(&paths, new("B", None), 16384).await.unwrap();
        mark_played(&paths, &a.id).await.unwrap();
        assert_eq!(list(&paths).await.unwrap()[0].id, "a");
    }
}

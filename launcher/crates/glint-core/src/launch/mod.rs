pub mod args;
pub mod jvm;
pub mod process;

use std::path::PathBuf;

pub use args::{LaunchCommand, LaunchContext, Session};
pub use process::{GameEvent, GameHandle, LogLine};

use crate::{
    Error, Result,
    download::ProgressFn,
    install,
    instance::Instance,
    java,
    meta::{fabric, mojang, mojang::VersionInfo},
    paths::Paths,
};

/// Everything resolved and on disk, ready to build a command line from.
#[derive(Debug, Clone)]
pub struct Prepared {
    pub version: VersionInfo,
    pub classpath: Vec<PathBuf>,
    pub java: PathBuf,
    pub java_major: u32,
}

/// Resolve metadata and make sure the game, Fabric and Java are installed.
///
/// Runs before every launch. When everything is already present it only checks files
/// exist, and it works offline as long as the metadata has been cached once.
pub async fn prepare(
    client: &reqwest::Client,
    paths: &Paths,
    instance: &Instance,
    concurrency: usize,
    progress: &ProgressFn,
) -> Result<Prepared> {
    let manifest = mojang::version_manifest(client, paths).await?;
    let entry = manifest.find(&instance.mc_version)?;
    if !entry.is_supported() {
        return Err(Error::Launch(format!(
            "Minecraft {} is older than 1.19, which Glint doesn't support",
            entry.id
        )));
    }
    let vanilla = mojang::version_info(client, paths, entry).await?;
    let profile = fabric::profile(
        client,
        paths,
        &instance.mc_version,
        &instance.loader_version,
    )
    .await?;
    let version = fabric::merge(vanilla, profile);
    let java_major = version.java_major();

    let java = match instance.java_path.as_deref().map(str::trim) {
        Some(custom) if !custom.is_empty() => {
            let path = PathBuf::from(custom);
            if !path.is_file() {
                return Err(Error::Java(format!(
                    "The custom Java path for this instance doesn't exist: {custom}"
                )));
            }
            path
        }
        _ => {
            java::ensure(client, paths, java_major, progress)
                .await?
                .executable
        }
    };

    let env = args::launch_env(instance);
    let installed = install::install(client, paths, &version, &env, concurrency, progress).await?;

    Ok(Prepared {
        version,
        classpath: installed.classpath,
        java,
        java_major,
    })
}

/// Build the command line for a prepared instance, creating its directories.
pub async fn command(
    paths: &Paths,
    instance: &Instance,
    prepared: &Prepared,
    session: &Session,
) -> Result<LaunchCommand> {
    let game_dir = paths.instance_game_dir(&instance.id);
    let natives_dir = paths.instance_natives_dir(&instance.id);
    tokio::fs::create_dir_all(&game_dir).await?;
    tokio::fs::create_dir_all(&natives_dir).await?;
    args::build(&LaunchContext {
        version: &prepared.version,
        instance,
        session,
        java: &prepared.java,
        java_major: prepared.java_major,
        classpath: &prepared.classpath,
        game_dir: &game_dir,
        natives_dir: &natives_dir,
        assets_dir: &paths.assets_dir(),
        libraries_dir: &paths.libraries_dir(),
    })
}

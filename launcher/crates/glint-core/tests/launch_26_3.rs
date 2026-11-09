//! Exercises metadata parsing, Fabric merging, library resolution and command building
//! against the real 26.3 and Fabric 0.20.1 metadata saved in `fixtures/`.

use std::{collections::HashMap, path::PathBuf};

use glint_core::{
    instance::{GcPreset, Instance},
    launch::{self, LaunchContext, Session},
    meta::{
        fabric::{self, FabricProfile},
        mojang::{self, VersionInfo},
        rules::Env,
    },
};

fn vanilla() -> VersionInfo {
    serde_json::from_str(include_str!("fixtures/26.3.json")).unwrap()
}

fn profile() -> FabricProfile {
    serde_json::from_str(include_str!("fixtures/fabric-0.20.1-26.3.json")).unwrap()
}

fn env(os_name: &'static str) -> Env {
    Env {
        os_name,
        arch: "x86_64",
        os_version: "10.0".into(),
        features: HashMap::new(),
    }
}

fn instance() -> Instance {
    Instance {
        id: "test".into(),
        name: "Test".into(),
        preset: None,
        mc_version: "26.3".into(),
        loader_version: "0.20.1".into(),
        memory_mb: 4096,
        gc: GcPreset::G1,
        jvm_args: "-Dglint.test=1".into(),
        java_path: None,
        width: None,
        height: None,
        mods: vec![],
        created_at: 0,
        last_played: None,
    }
}

#[test]
fn vanilla_metadata_parses() {
    let v = vanilla();
    assert_eq!(v.id, "26.3");
    assert_eq!(v.java_major(), 25);
    assert_eq!(v.main_class, "net.minecraft.client.main.Main");
    assert_eq!(v.asset_index.id, "34");
    assert_eq!(v.libraries.len(), 114);
    assert_eq!(v.vanilla_id(), "26.3");
}

#[test]
fn libraries_are_filtered_per_os() {
    let v = vanilla();
    let windows = mojang::resolve_libraries(&v, &env("windows")).unwrap();
    let linux = mojang::resolve_libraries(&v, &env("linux")).unwrap();

    assert!(
        windows
            .iter()
            .any(|l| l.path.ends_with("natives-windows.jar"))
    );
    assert!(!windows.iter().any(|l| l.path.contains("natives-linux")));
    assert!(!windows.iter().any(|l| l.path.contains("natives-macos")));
    assert!(linux.iter().any(|l| l.path.contains("natives-linux")));
    assert!(windows.len() < v.libraries.len());

    // Every Mojang library comes with a hash and size to verify against.
    assert!(windows.iter().all(|l| l.sha1.is_some() && l.size.is_some()));
}

#[test]
fn fabric_merges_over_vanilla() {
    let merged = fabric::merge(vanilla(), profile());
    assert_eq!(merged.id, "fabric-loader-0.20.1-26.3");
    assert_eq!(merged.vanilla_id(), "26.3");
    assert_eq!(
        merged.main_class,
        "net.fabricmc.loader.impl.launch.knot.KnotClient"
    );
    // Vanilla's jar, assets and Java requirement carry over.
    assert_eq!(merged.asset_index.id, "34");
    assert_eq!(merged.java_major(), 25);
    assert_eq!(merged.libraries.len(), 114 + 7);
    // Fabric's libraries lead the classpath.
    assert!(merged.libraries[0].name.starts_with("org.ow2.asm:asm:"));

    let libs = mojang::resolve_libraries(&merged, &env("windows")).unwrap();
    let loader = libs
        .iter()
        .find(|l| l.path.contains("fabric-loader"))
        .unwrap();
    assert_eq!(
        loader.url,
        "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.20.1/fabric-loader-0.20.1.jar"
    );
}

#[test]
fn builds_the_launch_command() {
    let merged = fabric::merge(vanilla(), profile());
    let instance = instance();
    let session = Session {
        username: "Steve".into(),
        uuid: "069a79f444e94726a5befca90e38aaf5".into(),
        access_token: "token-123".into(),
        user_type: "msa",
    };
    let classpath = [
        PathBuf::from("libs/a.jar"),
        PathBuf::from("versions/26.3.jar"),
    ];
    let command = launch::args::build(&LaunchContext {
        version: &merged,
        instance: &instance,
        session: &session,
        java: "java/bin/javaw.exe".as_ref(),
        java_major: 25,
        classpath: &classpath,
        game_dir: "inst/minecraft".as_ref(),
        natives_dir: "inst/natives".as_ref(),
        assets_dir: "assets".as_ref(),
        libraries_dir: "libraries".as_ref(),
    })
    .unwrap();
    let args = &command.args;
    let position = |needle: &str| {
        args.iter()
            .position(|a| a == needle)
            .unwrap_or_else(|| panic!("missing {needle} in {args:?}"))
    };
    let value_of = |flag: &str| args[position(flag) + 1].as_str();

    assert_eq!(command.program, PathBuf::from("java/bin/javaw.exe"));
    assert_eq!(command.cwd, PathBuf::from("inst/minecraft"));

    // JVM options, then the main class, then game options.
    let main = position("net.fabricmc.loader.impl.launch.knot.KnotClient");
    assert!(position("-Xmx4096M") < main);
    assert!(position("-XX:+UseG1GC") < main);
    assert!(position("-cp") < main);
    assert!(position("--username") > main);

    // The user's own JVM arguments come after everything else on the JVM side.
    assert_eq!(position("-Dglint.test=1"), main - 1);
    // Fabric's JVM argument survived the merge, spaces and all.
    assert!(args.iter().any(|a| a.starts_with("-DFabricMcEmu=")));

    let separator = if cfg!(windows) { ";" } else { ":" };
    assert_eq!(
        value_of("-cp"),
        ["libs/a.jar", "versions/26.3.jar"].join(separator)
    );
    assert_eq!(value_of("--username"), "Steve");
    assert_eq!(value_of("--uuid"), "069a79f444e94726a5befca90e38aaf5");
    assert_eq!(value_of("--accessToken"), "token-123");
    assert_eq!(value_of("--version"), "fabric-loader-0.20.1-26.3");
    assert_eq!(value_of("--assetIndex"), "34");
    assert_eq!(value_of("--gameDir"), "inst/minecraft");
    assert_eq!(value_of("--versionType"), "release");
    assert!(args.iter().any(|a| a == "-Dminecraft.launcher.brand=Glint"));
    assert!(
        args.iter()
            .any(|a| a.starts_with("-Djava.library.path=inst/natives"))
    );

    // Nothing is left unsubstituted, and options without a value are gone.
    assert!(!args.iter().any(|a| a.contains("${")));
    assert!(!args.iter().any(|a| a == "--xuid" || a == "--clientId"));
    // Feature-gated options stay off unless asked for.
    assert!(!args.iter().any(|a| a == "--demo" || a == "--width"));
    assert!(!args.iter().any(|a| a.starts_with("--quickPlay")));
}

#[test]
fn custom_resolution_adds_window_size() {
    let merged = fabric::merge(vanilla(), profile());
    let mut instance = instance();
    instance.width = Some(1280);
    instance.height = Some(720);
    let session = Session::offline("Dev");
    let command = launch::args::build(&LaunchContext {
        version: &merged,
        instance: &instance,
        session: &session,
        java: "java".as_ref(),
        java_major: 25,
        classpath: &[],
        game_dir: "g".as_ref(),
        natives_dir: "n".as_ref(),
        assets_dir: "a".as_ref(),
        libraries_dir: "l".as_ref(),
    })
    .unwrap();
    let args = command.args;
    let width = args.iter().position(|a| a == "--width").unwrap();
    assert_eq!(args[width + 1], "1280");
    let height = args.iter().position(|a| a == "--height").unwrap();
    assert_eq!(args[height + 1], "720");
}

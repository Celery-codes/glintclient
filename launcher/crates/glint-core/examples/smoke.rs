//! Headless end-to-end check: install Minecraft + Fabric + Java into a data directory,
//! launch with an offline profile, and stop once the game reports it has started.
//!
//!   cargo run -p glint-core --example smoke -- <data-dir> [mc-version]
//!
//! Development tool only. It uses an offline profile, which the launcher itself only
//! allows in debug builds.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use glint_core::{
    download::{Progress, ProgressFn},
    http_client,
    instance::{self, NewInstance},
    launch::{self, GameEvent, Session},
    meta::{fabric, mojang},
    paths::Paths,
    system,
};

/// Printed by the client once the window, renderer and resources are up.
const STARTED_MARKERS: &[&str] = &["Sound engine started", "Created: "];

#[tokio::main]
async fn main() -> glint_core::Result<()> {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().expect("usage: smoke <data-dir> [mc-version]");
    let paths = Paths::new(data_dir);
    let client = http_client()?;

    let manifest = mojang::version_manifest(&client, &paths).await?;
    let mc_version = args.next().unwrap_or(manifest.latest.release.clone());
    let loaders = fabric::loader_versions(&client, &paths, &mc_version).await?;
    let loader = fabric::recommended_loader(&loaders)
        .expect("Fabric has no loader for this version")
        .version
        .clone();
    println!("Minecraft {mc_version}, Fabric Loader {loader}");

    let existing = instance::list(&paths).await?;
    let instance = match existing.into_iter().find(|i| i.id == "smoke-test") {
        Some(instance) => instance,
        None => {
            let new = NewInstance {
                name: "Smoke test".into(),
                preset: Some("performance".into()),
                mc_version,
                loader_version: loader,
            };
            instance::create(&paths, new, system::total_memory_mb()).await?
        }
    };

    let last = Mutex::new((String::new(), Instant::now()));
    let progress: ProgressFn = Arc::new(move |p: Progress| {
        let mut last = last.lock().unwrap();
        let finished = p.done == p.total;
        if last.0 != p.stage || finished || last.1.elapsed() > Duration::from_secs(2) {
            println!("  {}: {}/{}", p.stage, p.done, p.total);
            *last = (p.stage.to_string(), Instant::now());
        }
    });

    let started = Instant::now();
    let prepared = launch::prepare(&client, &paths, &instance, 16, &progress).await?;
    println!(
        "Prepared in {:.1}s: {} classpath entries, Java {} at {}",
        started.elapsed().as_secs_f32(),
        prepared.classpath.len(),
        prepared.java_major,
        prepared.java.display()
    );

    let session = Session::offline("GlintDev");
    let command = launch::command(&paths, &instance, &prepared, &session).await?;
    println!("Launching ({} arguments)...", command.args.len());

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let mut handle = launch::process::spawn(command, move |event| {
        let _ = tx.send(event);
    })?;

    let launched = Instant::now();
    let mut saw_start = false;
    let mut fabric_line = None;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(180), rx.recv()).await;
        match event {
            Ok(Some(GameEvent::Log(lines))) => {
                for line in lines {
                    println!("    | {}", line.text);
                    if line.text.contains("Loading Minecraft") && line.text.contains("Fabric") {
                        fabric_line = Some(line.text.clone());
                    }
                    if !saw_start && STARTED_MARKERS.iter().any(|m| line.text.contains(m)) {
                        saw_start = true;
                        println!(
                            ">>> Game is up after {:.1}s; stopping it.",
                            launched.elapsed().as_secs_f32()
                        );
                        handle.kill();
                    }
                }
            }
            Ok(Some(GameEvent::Exit { code, killed })) => {
                println!(">>> Exited: code {code:?}, killed by us: {killed}");
                break;
            }
            Ok(None) => break,
            Err(_) => {
                println!(">>> No output for 3 minutes; stopping.");
                handle.kill();
            }
        }
    }

    println!(
        "Fabric banner: {}",
        fabric_line.as_deref().unwrap_or("<not seen>")
    );
    if saw_start && fabric_line.is_some() {
        println!("SMOKE TEST PASSED");
        Ok(())
    } else {
        println!("SMOKE TEST FAILED");
        std::process::exit(1);
    }
}

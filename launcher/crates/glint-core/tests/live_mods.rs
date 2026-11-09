//! Checks against the real Modrinth API. Ignored by default because they need the
//! network; run with `cargo test -p glint-core --test live_mods -- --ignored`.

use glint_core::{
    http_client,
    instance::{GcPreset, Instance, ModRef},
    mods::{SearchQuery, Source, index, manage::Context, modrinth},
    paths::Paths,
};

fn instance() -> Instance {
    Instance {
        id: "live".into(),
        name: "Live".into(),
        preset: None,
        mc_version: "26.3".into(),
        loader_version: "0.20.1".into(),
        memory_mb: 4096,
        gc: GcPreset::G1,
        jvm_args: String::new(),
        java_path: None,
        width: None,
        height: None,
        mods: vec![],
        created_at: 0,
        last_played: None,
    }
}

#[tokio::test]
#[ignore = "needs network"]
async fn search_finds_sodium_with_filters() {
    let client = http_client().unwrap();
    let query = SearchQuery {
        text: "sodium".into(),
        mc_version: Some("26.3".into()),
        fabric_only: true,
        category: Some("optimization".into()),
        page: 0,
        page_size: 10,
    };
    let page = modrinth::search(&client, &query).await.unwrap();
    assert!(page.total > 0);
    assert!(page.hits.iter().any(|h| h.slug == "sodium"));
    assert!(
        page.hits
            .iter()
            .all(|h| h.source == Source::Modrinth && h.downloadable)
    );
}

#[tokio::test]
#[ignore = "needs network"]
async fn install_resolves_dependencies_then_updates_and_adopts() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path());
    let client = http_client().unwrap();
    let instance = instance();
    let ctx = Context {
        client: &client,
        paths: &paths,
        instance: &instance,
        curseforge_key: None,
    };

    // Entity Culling requires Fabric API, which should come along automatically.
    let outcome = ctx
        .install(Source::Modrinth, "entityculling")
        .await
        .unwrap();
    assert!(outcome.failed.is_empty(), "{:?}", outcome.failed);
    let slugs: Vec<_> = outcome
        .installed
        .iter()
        .filter_map(|m| m.slug.clone())
        .collect();
    assert!(slugs.contains(&"entityculling".to_string()), "{slugs:?}");
    assert!(slugs.contains(&"fabric-api".to_string()), "{slugs:?}");
    for installed in &outcome.installed {
        assert!(index::file_path(&paths, "live", installed).is_file());
        assert!(
            !installed.title.ends_with(".jar"),
            "title should be the project name"
        );
    }

    // Zoomify ships one jar for Fabric and NeoForge and lists a NeoForge-only library
    // as required. That one must be skipped quietly; the Fabric ones must be installed.
    let zoomify = ctx.install(Source::Modrinth, "zoomify").await.unwrap();
    assert!(zoomify.failed.is_empty(), "{:?}", zoomify.failed);
    let slugs: Vec<_> = zoomify
        .installed
        .iter()
        .filter_map(|m| m.slug.clone())
        .collect();
    assert!(slugs.contains(&"zoomify".to_string()), "{slugs:?}");
    assert!(
        slugs.contains(&"fabric-language-kotlin".to_string()),
        "{slugs:?}"
    );
    assert!(
        !slugs.contains(&"kotlin-lang-forge".to_string()),
        "{slugs:?}"
    );

    // A mod with no build for this version fails with its name and a plain reason.
    let none = ctx
        .install(Source::Modrinth, "kotlin-lang-forge")
        .await
        .unwrap();
    assert!(none.installed.is_empty());
    assert_eq!(none.failed[0].project, "KotlinLangForge");
    assert!(
        none.failed[0]
            .message
            .contains("no build for Minecraft 26.3")
    );

    // Asking again installs nothing and says so.
    let again = ctx
        .install(Source::Modrinth, "entityculling")
        .await
        .unwrap();
    assert!(again.installed.is_empty());
    assert_eq!(again.already_installed.len(), 1);

    // A preset list skips what's there and installs the rest.
    let refs = ["fabric-api", "lithium", "no-such-mod-glint-test"].map(|p| ModRef {
        source: "modrinth".into(),
        project: p.into(),
    });
    let many = ctx.install_many(&refs).await.unwrap();
    assert_eq!(many.installed.len(), 1);
    assert_eq!(many.installed[0].slug.as_deref(), Some("lithium"));
    assert_eq!(many.failed.len(), 1);

    // Drop in an old Lithium build by hand: it should be recognised and offered an update.
    let lithium = many.installed[0].clone();
    let files = modrinth::files(&client, "lithium", "26.3").await.unwrap();
    let oldest = files.iter().min_by_key(|f| f.published.clone()).unwrap();
    if oldest.version_id != lithium.version_id.clone().unwrap() {
        std::fs::remove_file(index::file_path(&paths, "live", &lithium)).unwrap();
        let bytes = client
            .get(oldest.url.clone().unwrap())
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        std::fs::write(
            index::mods_dir(&paths, "live").join("hand-copied.jar"),
            &bytes,
        )
        .unwrap();

        let updates = ctx.check_updates().await.unwrap();
        let update = updates.iter().find(|u| u.filename == "hand-copied.jar");
        let update = update.expect("old build should have an update");
        assert_eq!(update.title, "Lithium");
        assert_eq!(update.current_version, oldest.version_name);

        let updated = ctx.update("hand-copied.jar").await.unwrap();
        assert_eq!(updated.version_id, lithium.version_id);
        assert!(
            !index::mods_dir(&paths, "live")
                .join("hand-copied.jar")
                .exists()
        );
        assert!(index::file_path(&paths, "live", &updated).is_file());
        assert!(ctx.check_updates().await.unwrap().is_empty());
    }
}

use factory_validator::{hex, stage_release_assets};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "infinity-factory-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ))
}

#[test]
fn staging_matches_every_catalog_entry_and_replaces_other_assets() {
    let root = root();
    let output = temp("release-assets");
    fs::create_dir_all(&output).expect("temporary output must be created");
    fs::write(output.join("bandcamp.bex"), b"rebuilt bytes")
        .expect("rebuilt package must be written");
    fs::write(output.join("untracked.bex"), b"rogue bytes")
        .expect("untracked package must be written");

    stage_release_assets(&root, &output).expect("canonical assets must stage");
    let catalog_bytes =
        fs::read(root.join("factory/bex-factory.json")).expect("catalog must exist");
    assert_eq!(
        fs::read(output.join("bex-factory.json")).expect("staged catalog must exist"),
        catalog_bytes
    );
    let catalog: Value = serde_json::from_slice(&catalog_bytes).expect("catalog must be JSON");
    let mut expected = vec!["bex-factory.json".to_string()];
    for plugin in catalog["plugins"]
        .as_array()
        .expect("plugins must be an array")
    {
        let name = plugin["asset_name"]
            .as_str()
            .expect("asset name must exist");
        let staged = fs::read(output.join(name)).expect("catalog asset must be staged");
        let fixture = fs::read(root.join("fixtures/packages").join(name))
            .expect("canonical fixture must exist");
        assert_eq!(staged, fixture, "{name} must match its tracked fixture");
        assert_eq!(hex(&staged), plugin["asset_sha256"]);
        assert_eq!(staged.len() as u64, plugin["asset_size"]);
        expected.push(name.to_string());
    }
    expected.sort();
    let mut actual: Vec<_> = fs::read_dir(&output)
        .expect("staged output must exist")
        .map(|entry| {
            entry
                .expect("entry must be readable")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    actual.sort();
    assert_eq!(actual, expected, "untracked release assets must be removed");
    fs::remove_dir_all(output).expect("temporary output must be removed");
}

#[test]
fn staging_rejects_fixture_bytes_not_bound_to_catalog() {
    let source = root();
    let root = temp("tampered-root");
    let output = temp("tampered-output");
    fs::create_dir_all(root.join("factory")).expect("factory directory must be created");
    fs::create_dir_all(root.join("fixtures/packages")).expect("fixture directory must be created");
    fs::copy(
        source.join("factory/bex-factory.json"),
        root.join("factory/bex-factory.json"),
    )
    .expect("catalog must be copied");
    for name in ["bandcamp.bex", "direct-url.bex", "instagram.bex"] {
        fs::copy(
            source.join("fixtures/packages").join(name),
            root.join("fixtures/packages").join(name),
        )
        .expect("fixture must be copied");
    }
    fs::write(
        root.join("fixtures/packages/bandcamp.bex"),
        b"rebuilt bytes",
    )
    .expect("fixture must be replaced");

    let error = stage_release_assets(&root, &output).expect_err("tampered fixture must fail");
    assert!(error.to_string().contains("bandcamp.bex"));
    assert!(
        !output.exists(),
        "failed staging must not create release output"
    );
    fs::remove_dir_all(root).expect("temporary root must be removed");
}

#[test]
fn factory_catalog_registers_facebook_public_resolver() {
    let catalog: Value = serde_json::from_slice(
        &fs::read(root().join("factory/bex-factory.json")).expect("catalog must exist"),
    )
    .expect("catalog must be JSON");
    let facebook = catalog["plugins"]
        .as_array()
        .expect("plugins must be an array")
        .iter()
        .find(|plugin| plugin["id"] == "media-url-resolver.infinity-factory.facebook")
        .expect("facebook resolver must be registered");

    assert_eq!(facebook["name"], "Facebook Public Resolver");
    assert_eq!(facebook["type"], "media-url-resolver");
    assert_eq!(facebook["version"], "4");
    assert_eq!(facebook["asset_name"], "facebook.bex");
    assert!(facebook["asset_size"].as_u64().is_some_and(|size| size > 0));
    assert_eq!(facebook["asset_sha256"].as_str().map(str::len), Some(64));
    assert_eq!(facebook["wit"]["package"], "component:media-url-resolver");
    assert_eq!(facebook["wit"]["version"], "2.0.0");
    assert_eq!(facebook["wit"]["world"], "media-url-resolver");
}

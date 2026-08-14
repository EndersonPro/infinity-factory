use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[test]
fn repository_manifest_and_workflows_are_safe() {
    let manifest: Value =
        serde_json::from_str(include_str!("../../../plugins/direct-url/manifest.json"))
            .expect("test fixture must be valid");
    assert_eq!(manifest["type"], "media-url-resolver");
    assert!(
        manifest["version"]
            .as_str()
            .expect("test fixture must be valid")
            .chars()
            .all(|ch| ch.is_ascii_digit())
    );
    assert!(
        manifest["host_site"]
            .as_array()
            .expect("test fixture must be valid")
            .iter()
            .all(|site| {
                site.as_str()
                    .is_some_and(|site| site.starts_with("https://"))
            })
    );
    for path in [
        "../../.github/workflows/ci.yml",
        "../../.github/workflows/release.yml",
    ] {
        let workflow = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
            .expect("test fixture must be valid");
        assert!(!workflow.contains("--skip-failures"));
        assert!(workflow.contains("cargo-component --version 0.21.1 --locked"));
        assert!(workflow.contains("ALLOW_WASIP2_FALLBACK: \"0\""));
    }
}

#[test]
fn runtime_repository_feed_matches_the_release_catalog_asset() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let feed: Value = serde_json::from_slice(
        &fs::read(root.join("repository.json")).expect("runtime repository feed must exist"),
    )
    .expect("runtime repository feed must be valid JSON");

    let repositories = feed["repositories"]
        .as_array()
        .expect("repositories must be an array");
    assert_eq!(repositories.len(), 1);
    assert_eq!(feed.as_object().map(|object| object.len()), Some(1));

    let repository = &repositories[0];
    assert_eq!(repository.as_object().map(|object| object.len()), Some(4));
    assert_eq!(repository["id"], 1);
    assert_eq!(repository["name"], "infinity-factory");
    assert_eq!(repository["install"], true);

    let catalog_url = repository["url"]
        .as_str()
        .expect("repository URL must be a string");
    assert_eq!(
        catalog_url,
        "https://github.com/EndersonPro/infinity-factory/releases/latest/download/bex-factory.json"
    );
    assert!(catalog_url.contains("github.com/EndersonPro/infinity-factory/"));
    let catalog_name = catalog_url
        .rsplit('/')
        .next()
        .expect("catalog URL must include an asset name");
    assert_eq!(catalog_name, "bex-factory.json");

    let release_workflow = fs::read_to_string(root.join(".github/workflows/release.yml"))
        .expect("release workflow must exist");
    assert!(release_workflow.contains("dist/bex-factory.json repository.json"));
    let build = release_workflow
        .find("./scripts/build-plugins.sh")
        .expect("release must validate source builds");
    let stage = release_workflow
        .find("./scripts/stage-release-assets.sh")
        .expect("release must stage canonical assets");
    let publish = release_workflow
        .find("gh release create")
        .expect("release must publish staged assets");
    assert!(build < stage && stage < publish);

    let build_script = fs::read_to_string(root.join("scripts/build-plugins.sh"))
        .expect("source build script must exist");
    assert!(build_script.contains("target/source-package-validation"));
    assert!(!build_script.contains("OUTPUT_DIR:=dist"));
}

#[test]
fn source_factory_binds_package_and_wit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value = serde_json::from_slice(
        &fs::read(root.join("factory/bex-factory.json")).expect("source index must exist"),
    )
    .expect("source index must be valid JSON");
    let package = fs::read(root.join("fixtures/packages/direct-url.bex"))
        .expect("package fixture must exist");
    let wit = fs::read(root.join("wit/media-url-resolver/wit/media-url-resolver.wit"))
        .expect("WIT must exist");
    let plugin = &index["plugins"][1];
    assert_eq!(
        plugin["id"],
        "media-url-resolver.infinity-factory.direct-url"
    );
    assert_eq!(plugin["asset_sha256"], hex(&package));
    assert_eq!(plugin["asset_size"], package.len());
    assert_eq!(plugin["wit"]["package"], "component:media-url-resolver");
    assert_eq!(plugin["wit"]["version"], "1.0.0");
    assert_eq!(plugin["wit"]["sha256"], hex(&wit));
    assert_eq!(plugin["wit"]["world"], "media-url-resolver");
}

#[test]
fn source_factory_binds_instagram_v2_package_and_wit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value = serde_json::from_slice(
        &fs::read(root.join("factory/bex-factory.json")).expect("source index must exist"),
    )
    .expect("source index must be valid JSON");
    let package = fs::read(root.join("fixtures/packages/instagram.bex"))
        .expect("instagram package fixture must exist");
    let wit = fs::read(root.join("wit/media-url-resolver-v2/wit/media-url-resolver.wit"))
        .expect("v2 WIT must exist");
    let plugin = index["plugins"]
        .as_array()
        .expect("source index must contain plugins")
        .iter()
        .find(|plugin| plugin["id"] == "media-url-resolver.infinity-factory.instagram")
        .expect("instagram must be registered");
    assert_eq!(
        plugin["id"],
        "media-url-resolver.infinity-factory.instagram"
    );
    assert_eq!(plugin["asset_sha256"], hex(&package));
    assert_eq!(plugin["asset_size"], package.len());
    assert_eq!(plugin["wit"]["package"], "component:media-url-resolver");
    assert_eq!(plugin["wit"]["version"], "2.0.0");
    assert_eq!(plugin["wit"]["sha256"], hex(&wit));
    assert_eq!(plugin["wit"]["world"], "media-url-resolver");

    let manifest: Value =
        serde_json::from_str(include_str!("../../../plugins/instagram/manifest.json"))
            .expect("instagram manifest must be JSON");
    assert_eq!(manifest["type"], "media-url-resolver");
    assert_eq!(manifest["resolver"], true);
    assert_eq!(manifest["abi"]["version"], "2.0.0");
    assert_eq!(manifest["network_policy"], "instagram-public-v1");
    assert!(
        manifest["host_site"]
            .as_array()
            .expect("host_site must be an array")
            .iter()
            .all(|site| site
                .as_str()
                .is_some_and(|site| site.starts_with("https://instagram.com")
                    || site.starts_with("https://www.instagram.com")))
    );
    assert!(
        manifest["keys_required"]
            .as_object()
            .is_some_and(serde_json::Map::is_empty),
        "keys_required must carry no secrets"
    );
}

#[test]
fn source_factory_binds_facebook_v2_package_and_wit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value = serde_json::from_slice(
        &fs::read(root.join("factory/bex-factory.json")).expect("source index must exist"),
    )
    .expect("source index must be valid JSON");
    let package = fs::read(root.join("fixtures/packages/facebook.bex"))
        .expect("facebook package fixture must exist");
    let wit = fs::read(root.join("wit/media-url-resolver-v2/wit/media-url-resolver.wit"))
        .expect("v2 WIT must exist");
    let plugin = index["plugins"]
        .as_array()
        .expect("source index must contain plugins")
        .iter()
        .find(|plugin| plugin["id"] == "media-url-resolver.infinity-factory.facebook")
        .expect("facebook must be registered");

    assert_eq!(plugin["asset_sha256"], hex(&package));
    assert_eq!(plugin["asset_size"], package.len());
    assert_eq!(plugin["wit"]["package"], "component:media-url-resolver");
    assert_eq!(plugin["wit"]["version"], "2.0.0");
    assert_eq!(plugin["wit"]["sha256"], hex(&wit));
    assert_eq!(plugin["wit"]["world"], "media-url-resolver");

    let manifest: Value =
        serde_json::from_str(include_str!("../../../plugins/facebook/manifest.json"))
            .expect("facebook manifest must be JSON");
    assert_eq!(manifest["type"], "media-url-resolver");
    assert_eq!(manifest["resolver"], true);
    assert_eq!(manifest["abi"]["version"], "2.0.0");
    assert_eq!(manifest["network_policy"], "facebook-public-v1");
}

#[test]
fn source_factory_binds_bandcamp_v2_package_and_wit() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value = serde_json::from_slice(
        &fs::read(root.join("factory/bex-factory.json")).expect("source index must exist"),
    )
    .expect("source index must be valid JSON");
    let package = fs::read(root.join("fixtures/packages/bandcamp.bex"))
        .expect("Bandcamp package fixture must exist");
    let wit = fs::read(root.join("wit/media-url-resolver-v2/wit/media-url-resolver.wit"))
        .expect("v2 WIT must exist");
    let plugin = &index["plugins"][0];
    assert_eq!(plugin["id"], "media-url-resolver.infinity-factory.bandcamp");
    assert_eq!(plugin["asset_sha256"], hex(&package));
    assert_eq!(plugin["asset_size"], package.len());
    assert_eq!(plugin["wit"]["package"], "component:media-url-resolver");
    assert_eq!(plugin["wit"]["version"], "2.0.0");
    assert_eq!(plugin["wit"]["sha256"], hex(&wit));
    assert_eq!(plugin["wit"]["world"], "media-url-resolver");

    let manifest: Value =
        serde_json::from_str(include_str!("../../../plugins/bandcamp/manifest.json"))
            .expect("Bandcamp manifest must be JSON");
    assert_eq!(manifest["network_policy"], "bandcamp-public-v1");
}

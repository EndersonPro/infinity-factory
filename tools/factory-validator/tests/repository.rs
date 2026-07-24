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
    let plugin = &index["plugins"][0];
    assert_eq!(plugin["asset_sha256"], hex(&package));
    assert_eq!(plugin["asset_size"], package.len());
    assert_eq!(plugin["wit"]["sha256"], hex(&wit));
    assert_eq!(plugin["wit"]["world"], "media-url-resolver");
}

use factory_validator::{hex, pack_all, validate_archive};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT: AtomicU64 = AtomicU64::new(0);

const V1_WIT: &str = "wit/media-url-resolver/wit/media-url-resolver.wit";
const V2_WIT: &str = "wit/media-url-resolver-v2/wit/media-url-resolver.wit";
const V1_EXPORT: &str = "component:media-url-resolver/resolver@1.0.0";
const V2_IMPORT: &str = "component:media-url-resolver/https-client@2.0.0";
const V2_EXPORT: &str = "component:media-url-resolver/resolver@2.0.0";

fn component(imports: &[&str], exports: &[&str]) -> Vec<u8> {
    let mut text = String::from("(component\n");
    for (index, name) in imports.iter().enumerate() {
        text += &format!("  (import \"{name}\" (instance $imp{index}))\n");
    }
    text += "  (component $inner)\n  (instance $exp (instantiate $inner))\n";
    for name in exports {
        text += &format!("  (export \"{name}\" (instance $exp))\n");
    }
    text += ")\n";
    wat::parse_str(text).expect("test fixture wat must be valid")
}

fn root() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("test fixture must be valid")
        .as_nanos();
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("infinity-factory-revision-{stamp}-{unique}"));
    fs::create_dir_all(path.join("wit/media-url-resolver/wit")).expect("workspace must build");
    fs::create_dir_all(path.join("wit/media-url-resolver-v2/wit")).expect("workspace must build");
    fs::create_dir_all(path.join("target/wasm32-wasip2/release")).expect("workspace must build");
    fs::write(path.join(V1_WIT), b"package test:v1;").expect("workspace must build");
    fs::write(path.join(V2_WIT), b"package test:v2;").expect("workspace must build");
    path
}

fn add_plugin(
    root: &std::path::Path,
    dir: &str,
    crate_name: &str,
    manifest: &serde_json::Value,
    wasm: &[u8],
) {
    let plugin = root.join("plugins").join(dir);
    fs::create_dir_all(&plugin).expect("workspace must build");
    fs::write(
        plugin.join("manifest.json"),
        serde_json::to_vec(manifest).expect("manifest must serialize"),
    )
    .expect("workspace must build");
    fs::write(
        root.join("target/wasm32-wasip2/release")
            .join(format!("{crate_name}.wasm")),
        wasm,
    )
    .expect("workspace must build");
}

fn v1_manifest(id: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "name": "Fixture", "version": "1", "type": "media-url-resolver"})
}

fn v2_manifest(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id, "name": "Fixture V2", "version": "2", "type": "media-url-resolver",
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "instagram-public-v1",
    })
}

#[test]
fn packages_exact_v1_legacy_manifest_reproducibly() {
    let root = root();
    add_plugin(
        &root,
        "direct-url",
        "direct_url",
        &v1_manifest("test.direct-url"),
        &component(&[], &[V1_EXPORT]),
    );
    let output = root.join("dist");
    pack_all(&root, &output).expect("exact v1 shape must package");
    let first = fs::read(output.join("direct-url.bex")).expect("asset must exist");
    pack_all(&root, &output).expect("must repack");
    let second = fs::read(output.join("direct-url.bex")).expect("asset must exist");
    assert_eq!(first, second, "packaging must be byte-reproducible");
    validate_archive(&output.join("direct-url.bex")).expect("archive must validate");
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("bex-factory.json")).expect("index must exist"),
    )
    .expect("index must be JSON");
    assert_eq!(
        index["plugins"][0]["wit"]["package"],
        "component:media-url-resolver"
    );
    assert_eq!(index["plugins"][0]["wit"]["version"], "1.0.0");
    assert_eq!(index["plugins"][0]["wit"]["world"], "media-url-resolver");
    let wit = fs::read(root.join(V1_WIT)).expect("wit must exist");
    assert_eq!(index["plugins"][0]["wit"]["sha256"], hex(&wit));
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn packages_component_built_for_wasip1() {
    let root = root();
    add_plugin(
        &root,
        "direct-url",
        "direct_url",
        &v1_manifest("test.direct-url"),
        &component(&[], &[V1_EXPORT]),
    );
    let wasip1 = root.join("target/wasm32-wasip1/release/direct_url.wasm");
    fs::create_dir_all(wasip1.parent().expect("wasip1 path must have a parent"))
        .expect("wasip1 target directory must be created");
    fs::rename(
        root.join("target/wasm32-wasip2/release/direct_url.wasm"),
        &wasip1,
    )
    .expect("component must be moved to wasip1 target");

    let output = root.join("dist");
    pack_all(&root, &output).expect("wasip1 component must package");
    validate_archive(&output.join("direct-url.bex")).expect("archive must validate");
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn packages_exact_v2_manifest_with_declared_abi_and_policy() {
    let root = root();
    add_plugin(
        &root,
        "instagram",
        "instagram",
        &v2_manifest("test.instagram"),
        &component(&[V2_IMPORT], &[V2_EXPORT]),
    );
    let output = root.join("dist");
    pack_all(&root, &output).expect("exact v2 shape must package");
    let index: serde_json::Value = serde_json::from_slice(
        &fs::read(output.join("bex-factory.json")).expect("index must exist"),
    )
    .expect("index must be JSON");
    assert_eq!(index["plugins"][0]["wit"]["version"], "2.0.0");
    let wit = fs::read(root.join(V2_WIT)).expect("wit must exist");
    assert_eq!(index["plugins"][0]["wit"]["sha256"], hex(&wit));
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn adding_a_v2_plugin_does_not_change_v1_packaged_bytes() {
    let root = root();
    add_plugin(
        &root,
        "direct-url",
        "direct_url",
        &v1_manifest("test.direct-url"),
        &component(&[], &[V1_EXPORT]),
    );
    let output = root.join("dist");
    pack_all(&root, &output).expect("v1-only workspace must package");
    let alone = fs::read(output.join("direct-url.bex")).expect("asset must exist");

    add_plugin(
        &root,
        "instagram",
        "instagram",
        &v2_manifest("test.instagram"),
        &component(&[V2_IMPORT], &[V2_EXPORT]),
    );
    pack_all(&root, &output).expect("mixed workspace must package");
    let alongside = fs::read(output.join("direct-url.bex")).expect("asset must exist");
    assert_eq!(alone, alongside, "v1 artifact must stay byte-identical");
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn rejects_v2_declared_manifest_with_v1_shaped_component() {
    let root = root();
    add_plugin(
        &root,
        "instagram",
        "instagram",
        &v2_manifest("test.instagram"),
        &component(&[], &[V1_EXPORT]),
    );
    assert!(pack_all(&root, &root.join("dist")).is_err());
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn rejects_legacy_manifest_with_v2_shaped_component() {
    let root = root();
    add_plugin(
        &root,
        "direct-url",
        "direct_url",
        &v1_manifest("test.direct-url"),
        &component(&[V2_IMPORT], &[V2_EXPORT]),
    );
    assert!(pack_all(&root, &root.join("dist")).is_err());
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn rejects_unknown_declared_revision_before_packaging() {
    let root = root();
    let mut manifest = v2_manifest("test.instagram");
    manifest["abi"]["version"] = serde_json::json!("9.9.9");
    add_plugin(
        &root,
        "instagram",
        "instagram",
        &manifest,
        &component(&[V2_IMPORT], &[V2_EXPORT]),
    );
    assert!(pack_all(&root, &root.join("dist")).is_err());
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

#[test]
fn rejects_mixed_archive_when_one_plugin_mismatches() {
    let root = root();
    add_plugin(
        &root,
        "direct-url",
        "direct_url",
        &v1_manifest("test.direct-url"),
        &component(&[], &[V1_EXPORT]),
    );
    add_plugin(
        &root,
        "instagram",
        "instagram",
        &v2_manifest("test.instagram"),
        &component(&[], &[V2_EXPORT]), // missing required https-client import
    );
    assert!(pack_all(&root, &root.join("dist")).is_err());
    fs::remove_dir_all(root).expect("cleanup must succeed");
}

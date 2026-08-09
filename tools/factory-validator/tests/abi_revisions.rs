use factory_validator::{ACCEPTED, accepted_for_manifest};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    fs::read_to_string(root().join(path)).expect("contract artifact must exist")
}

fn hex(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn assert_policy(policy: &Value) {
    assert_eq!(policy["id"], "instagram-public-v1");
    assert_eq!(policy["abi"]["package"], "component:media-url-resolver");
    assert_eq!(policy["abi"]["version"], "2.0.0");
    assert_eq!(policy["abi"]["world"], "media-url-resolver");
    assert_eq!(
        policy["operations"],
        serde_json::json!(["get", "post-public-graphql"])
    );
    assert_eq!(
        policy["allowed_hosts"],
        serde_json::json!(["instagram.com", "www.instagram.com"])
    );
    assert_eq!(
        policy["graphql_endpoint"],
        "https://www.instagram.com/api/graphql"
    );
    assert_eq!(
        policy["get_headers"],
        serde_json::json!(["accept", "accept-language"])
    );
    assert_eq!(
        policy["response_headers"],
        serde_json::json!([
            "cache-control",
            "content-length",
            "content-type",
            "etag",
            "last-modified",
            "retry-after"
        ])
    );
    let limits = &policy["limits"];
    for (key, value) in [
        ("url_bytes", 2048),
        ("lsd_bytes", 256),
        ("friendly_name_bytes", 128),
        ("doc_id_bytes", 64),
        ("variables_bytes", 32768),
        ("form_body_bytes", 65536),
        ("response_body_bytes", 4194304),
        ("deadline_ms", 10000),
        ("redirects", 3),
        ("cookies", 32),
        ("cookie_bytes", 4096),
        ("cookie_jar_bytes", 16384),
    ] {
        assert_eq!(limits[key], value, "wrong {key}");
    }
}

#[test]
fn abi_v2_policy_identity() {
    let v1 = read("wit/media-url-resolver/wit/media-url-resolver.wit");
    let v1_revision = read("wit/media-url-resolver/REVISION");
    let v1_sha = v1_revision
        .lines()
        .find_map(|line| line.strip_prefix("sha256="))
        .expect("v1 revision must contain sha256");
    assert_eq!(hex(&v1), v1_sha);

    let v2 = read("wit/media-url-resolver-v2/wit/media-url-resolver.wit");
    for token in [
        "package component:media-url-resolver@2.0.0;",
        "interface https-client",
        "get: func",
        "post-public-graphql: func",
        "import https-client;",
        "export resolver;",
    ] {
        assert!(v2.contains(token), "missing {token}");
    }
    assert!(!v2.contains("method:"));
    assert_eq!(v2.matches("import ").count(), 1);
    assert_eq!(v2.matches(": func").count(), 3);

    let policy_text = read("compatibility/media-url-resolver-v2-policy.json");
    assert_eq!(
        hex(&policy_text),
        "2fa56985022a3ab2979f007b06a5d35850622a7734d572a4fc966434678e74c0"
    );
    let policy: Value = serde_json::from_str(&policy_text).expect("policy must be JSON");
    assert_policy(&policy);
    let v2_sha = hex(&v2);
    assert_eq!(policy["abi"]["wit_sha256"], v2_sha);
    assert!(
        read("wit/media-url-resolver-v2/REVISION")
            .lines()
            .any(|line| line == format!("sha256={v2_sha}"))
    );

    let vectors_text = read("compatibility/v2/abi-identity-vectors.json");
    assert_eq!(
        hex(&vectors_text),
        "c3432c9049edf50f5461a04960261bdbc73f23b0473c8c9b91f42585a75998ba"
    );
    let vectors: Value = serde_json::from_str(&vectors_text).expect("vectors must be JSON");
    assert!(
        vectors["valid"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    );
    assert!(
        vectors["invalid"]
            .as_array()
            .is_some_and(|items| items.len() >= 8)
    );
    assert!(
        read("compatibility/media-url-resolver-v2-host-contract.md")
            .contains("external native host")
    );
}

#[test]
fn accepted_revision_table_binds_exact_on_disk_wit() {
    for revision in ACCEPTED {
        let wit = read(revision.wit_path);
        assert_eq!(
            wit.lines().next(),
            Some(format!(
                "package {}@{};",
                revision.package, revision.version
            ))
            .as_deref()
        );
        assert!(!wit.is_empty());
    }
    assert_eq!(ACCEPTED.len(), 2, "only v1 and v2 are accepted revisions");
}

#[test]
fn resolves_the_real_direct_url_manifest_as_legacy_v1() {
    let manifest: Value = serde_json::from_str(&read("plugins/direct-url/manifest.json"))
        .expect("manifest must be JSON");
    assert!(
        manifest.get("abi").is_none(),
        "direct-url stays legacy-shaped"
    );
    let revision = accepted_for_manifest(&manifest).expect("legacy manifest must resolve to v1");
    assert_eq!(revision.version, "1.0.0");
}

#[test]
fn resolves_an_exact_v2_manifest_and_rejects_unknown_or_mismatched_ones() {
    let exact = serde_json::json!({
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "instagram-public-v1",
    });
    let revision = accepted_for_manifest(&exact).expect("exact v2 manifest must resolve");
    assert_eq!(revision.version, "2.0.0");
    assert_eq!(
        revision.wit_path,
        "wit/media-url-resolver-v2/wit/media-url-resolver.wit"
    );

    let unknown = serde_json::json!({
        "abi": {"package": "component:media-url-resolver", "version": "3.0.0", "world": "media-url-resolver"},
    });
    assert!(accepted_for_manifest(&unknown).is_err());

    let mismatched_policy = serde_json::json!({
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "other-policy",
    });
    assert!(accepted_for_manifest(&mismatched_policy).is_err());

    let mismatched_package = serde_json::json!({
        "abi": {"package": "component:other", "version": "2.0.0", "world": "media-url-resolver"},
    });
    assert!(accepted_for_manifest(&mismatched_package).is_err());
}

#[test]
fn resolves_bandcamp_manifest_after_host_policy_support() {
    let bandcamp = serde_json::json!({
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "bandcamp-public-v1",
    });
    let revision = accepted_for_manifest(&bandcamp).expect("Bandcamp v2 policy must resolve");
    assert_eq!(revision.version, "2.0.0");
}

#[test]
fn rejects_policy_identity_drift() {
    let source: Value =
        serde_json::from_str(&read("compatibility/media-url-resolver-v2-policy.json"))
            .expect("policy must be JSON");
    for pointer in [
        "/abi/version",
        "/abi/world",
        "/graphql_endpoint",
        "/limits/deadline_ms",
    ] {
        let mut changed = source.clone();
        changed
            .pointer_mut(pointer)
            .expect("pointer must exist")
            .clone_from(&Value::Null);
        assert!(std::panic::catch_unwind(|| assert_policy(&changed)).is_err());
    }
}

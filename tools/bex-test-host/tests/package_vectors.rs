//! Package-validation vectors: real non-executing packages, the strict
//! manifest/policy/WIT/catalog matrices, and the synthetic Bandcamp policy
//! identity bound to the canonical v2 WIT digest (no canonical Bandcamp files
//! are imported — the host composes that identity itself).

use bex_test_host::{Command, contracts::HostError, inspect_package, package, validate_package};
use serde_json::json;
use std::{cell::Cell, io::Read, path::PathBuf, rc::Rc};

/// Yields up to its byte budget and counts how many the consumer pulled.
struct CountingReader(usize, Rc<Cell<usize>>);

impl Read for CountingReader {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        let n = b.len().min(self.0);
        self.0 -= n;
        self.1.set(self.1.get() + n);
        Ok(n)
    }
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pkg(name: &str) -> PathBuf {
    root().join("fixtures/packages").join(name)
}

fn catalog() -> PathBuf {
    root().join("factory/bex-factory.json")
}

fn err_code<T>(result: Result<T, HostError>) -> u8 {
    result.err().expect("expected error").code()
}

#[test]
fn validate_real_direct_url_package_succeeds_without_catalog() {
    let report = validate_package(&pkg("direct-url.bex"), None).expect("direct-url must validate");
    assert_eq!(report.command, Some(Command::Validate));
    let result = report.result.expect("success must carry result");
    assert_eq!(result.package.version, "1");
    assert_eq!(result.package.wit.version, "1.0.0");
    assert_eq!(result.package.wit.sha256, package::canonical_v1_digest());
    assert!(
        result.package.network_policy.is_none(),
        "v1 carries no network policy"
    );
    assert!(!result.execution.supported);
    assert_eq!(result.execution.status, "deferred");
}

#[test]
fn inspect_real_instagram_package_with_catalog_succeeds() {
    let report =
        inspect_package(&pkg("instagram.bex"), Some(&catalog())).expect("instagram must inspect");
    assert_eq!(report.command, Some(Command::Inspect));
    let result = report.result.expect("success must carry result");
    assert_eq!(result.package.version, "2");
    assert_eq!(
        result.package.network_policy.as_deref(),
        Some("instagram-public-v1")
    );
    assert_eq!(result.package.wit.sha256, package::canonical_v2_digest());
    // catalog bound the full tuple: emit the package itself matched.
    assert_eq!(result.package.asset_name, "instagram.bex");
}

#[test]
fn inspect_real_bandcamp_package_with_catalog_succeeds() {
    let report =
        inspect_package(&pkg("bandcamp.bex"), Some(&catalog())).expect("Bandcamp must inspect");
    let result = report.result.expect("success must carry result");
    assert_eq!(result.package.version, "2");
    assert_eq!(
        result.package.network_policy.as_deref(),
        Some("bandcamp-public-v1")
    );
    assert_eq!(result.package.wit.sha256, package::canonical_v2_digest());
    assert_eq!(result.package.asset_name, "bandcamp.bex");
}

#[test]
fn catalog_is_optional_for_validate() {
    assert!(validate_package(&pkg("instagram.bex"), None).is_ok());
    assert!(validate_package(&pkg("instagram.bex"), Some(&catalog())).is_ok());
}

#[test]
fn truncated_package_is_rejected_with_exit_three() {
    let full = std::fs::read(pkg("direct-url.bex")).expect("fixture must exist");
    let dir = std::env::temp_dir().join("bex-truncated.bex");
    std::fs::write(&dir, &full[..full.len() / 4]).expect("write must succeed");
    assert_eq!(
        err_code(validate_package(&dir, None)),
        HostError::Package.code()
    );
    std::fs::remove_file(&dir).ok();
}

#[test]
fn synthetic_v2_instagram_manifest_binds_canonical_v2_wit_digest() {
    let manifest = json!({
        "manifest_version": "1.0",
        "id": "media-url-resolver.infinity-factory.instagram",
        "name": "Instagram Public Resolver",
        "version": "2",
        "type": "media-url-resolver",
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "instagram-public-v1",
    });
    let identity =
        package::policy_for_manifest(&manifest).expect("instagram v2 manifest must resolve");
    assert_eq!(identity.version, "2");
    assert_eq!(identity.policy, Some("instagram-public-v1"));
    assert_eq!(identity.wit_digest, package::canonical_v2_digest());
}

#[test]
fn synthetic_v2_bandcamp_manifest_binds_same_canonical_v2_wit_digest() {
    let manifest = json!({
        "manifest_version": "1.0",
        "id": "media-url-resolver.infinity-factory.bandcamp",
        "name": "Bandcamp Public Resolver",
        "version": "2",
        "type": "media-url-resolver",
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "bandcamp-public-v1",
    });
    let identity =
        package::policy_for_manifest(&manifest).expect("bandcamp v2 identity must compose");
    assert_eq!(identity.version, "2");
    assert_eq!(identity.policy, Some("bandcamp-public-v1"));
    assert_eq!(identity.wit_digest, package::canonical_v2_digest());
}

#[test]
fn synthetic_v1_manifest_binds_canonical_v1_wit_digest_without_policy() {
    let manifest = json!({
        "manifest_version": "1.0",
        "id": "media-url-resolver.infinity-factory.direct-url",
        "name": "Direct URL Resolver",
        "version": "1",
        "type": "media-url-resolver",
    });
    let identity = package::policy_for_manifest(&manifest).expect("v1 manifest must resolve");
    assert_eq!(identity.version, "1");
    assert!(identity.policy.is_none());
    assert_eq!(identity.wit_digest, package::canonical_v1_digest());
}

#[test]
fn unknown_network_policy_is_rejected_as_package_error() {
    let manifest = json!({
        "manifest_version": "1.0",
        "id": "x", "name": "n", "version": "2", "type": "media-url-resolver",
        "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
        "network_policy": "tumblr-public-v1",
    });
    assert_eq!(
        err_code(package::policy_for_manifest(&manifest)),
        HostError::Package.code()
    );
}

#[test]
fn manifest_unknown_abi_version_is_rejected() {
    let manifest = json!({
        "manifest_version": "1.0",
        "id": "x", "name": "n", "version": "9", "type": "media-url-resolver",
    });
    assert_eq!(
        err_code(package::policy_for_manifest(&manifest)),
        HostError::Package.code()
    );
}

#[test]
fn manifest_unknown_top_level_field_is_rejected() {
    let mut manifest = json!({
        "manifest_version": "1.0",
        "id": "x", "name": "n", "version": "1", "type": "media-url-resolver",
    });
    manifest["unexpected_field"] = json!("boom");
    assert_eq!(
        err_code(package::policy_for_manifest(&manifest)),
        HostError::Package.code()
    );
}

#[test]
fn catalog_rejects_zero_or_multiple_matches() {
    // A catalog that points at a different asset SHA-256 must fail the tuple,
    // proving the binding is exact and not satisfied by ID alone.
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog()).expect("catalog must read"))
            .expect("catalog must parse");
    value["plugins"][2]["asset_sha256"] = json!("0".repeat(64));
    let path = std::env::temp_dir().join("bex-bad-catalog.json");
    std::fs::write(&path, value.to_string()).expect("write must succeed");
    assert_eq!(
        err_code(validate_package(&pkg("instagram.bex"), Some(&path))),
        HostError::Package.code()
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn catalog_rejects_when_no_entry_matches_the_id() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog()).expect("catalog must read"))
            .expect("catalog must parse");
    value["plugins"][2]["id"] = json!("media-url-resolver.infinity-factory.missing");
    let path = std::env::temp_dir().join("bex-zero-match.json");
    std::fs::write(&path, value.to_string()).expect("write must succeed");
    assert_eq!(
        err_code(validate_package(&pkg("instagram.bex"), Some(&path))),
        HostError::Package.code()
    );
    std::fs::remove_file(&path).ok();
}

/// The pre-decode boundary accepts N and rejects N+1/N+2 while never consuming
/// or retaining beyond N+1; the existing real-package suite triangulates below N.
#[test]
fn bounded_reader_consumes_at_most_limit_plus_one() {
    let n = package::COMPRESSED_MAX;
    for (size, accept) in [(n, true), (n + 1, false), (n + 2, false)] {
        let consumed = Rc::new(Cell::new(0usize));
        let result = package::read_compressed_bytes(CountingReader(size, Rc::clone(&consumed)));
        assert_eq!(accept, result.is_ok(), "size {size}");
        assert!(consumed.get() <= n + 1, "size {size}");
        assert!(result.as_ref().map_or(0, Vec::len) <= n, "size {size}");
    }
}

#[test]
fn catalog_rejects_duplicate_plugin_ids() {
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(catalog()).expect("catalog must read"))
            .expect("catalog must parse");
    let duplicate = value["plugins"][2].clone();
    value["plugins"].as_array_mut().unwrap().push(duplicate);
    let path = std::env::temp_dir().join("bex-duplicate.json");
    std::fs::write(&path, value.to_string()).expect("write must succeed");
    assert_eq!(
        err_code(validate_package(&pkg("instagram.bex"), Some(&path))),
        HostError::Package.code()
    );
    std::fs::remove_file(&path).ok();
}

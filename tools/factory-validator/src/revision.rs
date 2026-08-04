use serde_json::Value;
use wasmparser::{Parser, Payload};

/// An exact, packaged ABI revision: the manifest identity it is bound to,
/// the canonical WIT source it must be packaged with, and the exact
/// component-level import/export it must decode to. Nothing here is
/// inferred; every field is compared for byte-exact equality.
pub struct AcceptedRevision {
    pub package: &'static str,
    pub version: &'static str,
    pub world: &'static str,
    pub wit_path: &'static str,
    /// Exact qualified host import the component must have, or `None` when
    /// the revision must have zero `component:media-url-resolver/*` imports.
    pub import: Option<&'static str>,
    pub export: &'static str,
    /// Exact `network_policy` manifest fields accepted for this revision.
    /// An empty list means the revision carries no host network policy.
    pub network_policies: &'static [&'static str],
}

pub static V1: AcceptedRevision = AcceptedRevision {
    package: "component:media-url-resolver",
    version: "1.0.0",
    world: "media-url-resolver",
    wit_path: "wit/media-url-resolver/wit/media-url-resolver.wit",
    import: None,
    export: "component:media-url-resolver/resolver@1.0.0",
    network_policies: &[],
};

pub static V2: AcceptedRevision = AcceptedRevision {
    package: "component:media-url-resolver",
    version: "2.0.0",
    world: "media-url-resolver",
    wit_path: "wit/media-url-resolver-v2/wit/media-url-resolver.wit",
    import: Some("component:media-url-resolver/https-client@2.0.0"),
    export: "component:media-url-resolver/resolver@2.0.0",
    network_policies: &["instagram-public-v1", "bandcamp-public-v1"],
};

/// Every revision the packer accepts. Fail closed: any manifest, package,
/// version, world, import, export, or policy combination not in this exact
/// table is rejected, never silently substituted or defaulted.
pub static ACCEPTED: [&AcceptedRevision; 2] = [&V1, &V2];

const PACKAGE_PREFIX: &str = "component:media-url-resolver/";

/// Resolve the accepted revision a plugin manifest declares.
///
/// Manifests without an `abi` object are a candidate legacy v1 and are
/// returned as `V1`, but the caller MUST still bind the produced component
/// with `validate_component_revision` — a legacy manifest never gets a free
/// pass just because it omits the field; it must still decode to exact v1
/// shape or packaging fails closed.
pub fn accepted_for_manifest(manifest: &Value) -> Result<&'static AcceptedRevision, String> {
    let Some(abi) = manifest.get("abi") else {
        return Ok(&V1);
    };
    let package = abi["package"]
        .as_str()
        .ok_or("manifest abi.package missing")?;
    let version = abi["version"]
        .as_str()
        .ok_or("manifest abi.version missing")?;
    let world = abi["world"].as_str().ok_or("manifest abi.world missing")?;
    let revision = ACCEPTED
        .into_iter()
        .find(|item| item.package == package && item.version == version && item.world == world)
        .ok_or_else(|| format!("unknown ABI revision {package}@{version} world {world}"))?;
    if !revision.network_policies.is_empty() {
        let declared = manifest["network_policy"]
            .as_str()
            .ok_or("manifest network_policy missing")?;
        if !revision.network_policies.contains(&declared) {
            return Err(format!(
                "network_policy {declared} is not accepted for {package}@{version}"
            ));
        }
    }
    Ok(revision)
}

/// Parsed import/export shape of a built component, scoped to the
/// `component:media-url-resolver/*` package plus the unrelated host imports
/// (e.g. WASI) the host reports as `unprovided` without rejecting them. This
/// extraction is policy-neutral: it carries no network-policy identity.
pub struct ComponentShapeReport {
    pub package_imports: Vec<String>,
    pub package_exports: Vec<String>,
    pub unprovided_imports: Vec<String>,
}

/// The neutral component shape an accepted revision expects, independent of
/// any host policy binding. Hosts compose their own Instagram/Bandcamp policy
/// identities on top of this shared, policy-neutral skeleton.
pub struct AbiShape {
    pub package: &'static str,
    pub version: &'static str,
    pub world: &'static str,
    pub wit_path: &'static str,
    pub package_imports: Vec<&'static str>,
    pub package_exports: Vec<&'static str>,
}

/// Decode a component's own top-level import/export names via the
/// WebAssembly component-model binary format (not text/WIT matching).
fn component_bindings(wasm: &[u8]) -> Result<(Vec<String>, Vec<String>), String> {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(wasm) {
        match payload.map_err(|error| error.to_string())? {
            Payload::ComponentImportSection(section) => {
                for import in section {
                    imports.push(import.map_err(|error| error.to_string())?.name.name.into());
                }
            }
            Payload::ComponentExportSection(section) => {
                for export in section {
                    exports.push(export.map_err(|error| error.to_string())?.name.name.into());
                }
            }
            _ => {}
        }
    }
    Ok((imports, exports))
}

/// Inspect a built component into a policy-neutral shape report. Unrelated
/// imports (WASI or any non-`component:media-url-resolver/*` import) are
/// reported as `unprovided`, never silently rejected.
pub fn component_shape(wasm: &[u8]) -> Result<ComponentShapeReport, String> {
    let (imports, exports) = component_bindings(wasm)?;
    let package_imports: Vec<String> = imports
        .iter()
        .filter(|name| name.starts_with(PACKAGE_PREFIX))
        .cloned()
        .collect();
    let package_exports: Vec<String> = exports
        .iter()
        .filter(|name| name.starts_with(PACKAGE_PREFIX))
        .cloned()
        .collect();
    let unprovided_imports: Vec<String> = imports
        .iter()
        .filter(|name| !name.starts_with(PACKAGE_PREFIX))
        .cloned()
        .collect();
    Ok(ComponentShapeReport {
        package_imports,
        package_exports,
        unprovided_imports,
    })
}

/// Project an accepted revision into the policy-neutral shape it expects. The
/// returned `AbiShape` is the shared skeleton the packer and the test-only
/// host both bind against.
pub fn accepted_shape(revision: &AcceptedRevision) -> AbiShape {
    AbiShape {
        package: revision.package,
        version: revision.version,
        world: revision.world,
        wit_path: revision.wit_path,
        package_imports: revision.import.into_iter().collect(),
        package_exports: vec![revision.export],
    }
}

/// Bind a built component to the accepted revision its manifest declared.
/// Fails closed on any extra, missing, or substituted package-scoped
/// import/export; unrelated host imports (e.g. WASI) are ignored.
pub fn validate_component_revision(wasm: &[u8], revision: &AcceptedRevision) -> Result<(), String> {
    let report = component_shape(wasm)?;
    let expected = accepted_shape(revision);
    let import_ok = report.package_imports == expected.package_imports;
    let export_ok = report.package_exports == expected.package_exports;
    (import_ok && export_ok).then_some(()).ok_or_else(|| {
        format!(
            "component does not match accepted revision {}@{}",
            revision.package, revision.version
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_legacy_manifest_without_abi_as_candidate_v1() {
        let manifest = serde_json::json!({"id": "x"});
        let revision = accepted_for_manifest(&manifest).expect("legacy manifest must resolve");
        assert_eq!(revision.version, "1.0.0");
    }

    #[test]
    fn accepts_exact_v2_manifest_with_matching_policy() {
        let manifest = serde_json::json!({
            "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
            "network_policy": "instagram-public-v1",
        });
        let revision = accepted_for_manifest(&manifest).expect("exact v2 manifest must resolve");
        assert_eq!(revision.version, "2.0.0");
    }

    #[test]
    fn accepts_exact_v2_bandcamp_policy() {
        let manifest = serde_json::json!({
            "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
            "network_policy": "bandcamp-public-v1",
        });
        let revision = accepted_for_manifest(&manifest).expect("Bandcamp v2 policy must resolve");
        assert_eq!(revision.version, "2.0.0");
    }

    #[test]
    fn rejects_unknown_revision() {
        let manifest = serde_json::json!({
            "abi": {"package": "component:media-url-resolver", "version": "9.9.9", "world": "media-url-resolver"},
        });
        assert!(accepted_for_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_v2_manifest_with_mismatched_policy() {
        let manifest = serde_json::json!({
            "abi": {"package": "component:media-url-resolver", "version": "2.0.0", "world": "media-url-resolver"},
            "network_policy": "unexpected-policy",
        });
        assert!(accepted_for_manifest(&manifest).is_err());
    }

    #[test]
    fn rejects_declared_v1_with_wrong_world() {
        let manifest = serde_json::json!({
            "abi": {"package": "component:media-url-resolver", "version": "1.0.0", "world": "other-world"},
        });
        assert!(accepted_for_manifest(&manifest).is_err());
    }

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

    #[test]
    fn validates_exact_v1_and_v2_component_shapes() {
        let v1 = component(&[], &["component:media-url-resolver/resolver@1.0.0"]);
        assert!(validate_component_revision(&v1, &V1).is_ok());
        let v2 = component(
            &["component:media-url-resolver/https-client@2.0.0"],
            &["component:media-url-resolver/resolver@2.0.0"],
        );
        assert!(validate_component_revision(&v2, &V2).is_ok());
    }

    #[test]
    fn rejects_cross_revision_and_extra_import_components() {
        // v2-shaped component bound against V1: extra unexpected import.
        let v2 = component(
            &["component:media-url-resolver/https-client@2.0.0"],
            &["component:media-url-resolver/resolver@2.0.0"],
        );
        assert!(validate_component_revision(&v2, &V1).is_err());
        // v1-shaped component bound against V2: missing required import.
        let v1 = component(&[], &["component:media-url-resolver/resolver@1.0.0"]);
        assert!(validate_component_revision(&v1, &V2).is_err());
        // exact revision, but exported under the wrong version.
        let stale = component(&[], &["component:media-url-resolver/resolver@9.0.0"]);
        assert!(validate_component_revision(&stale, &V1).is_err());
        // duplicate package-scoped exports.
        let duplicate = component(
            &[],
            &[
                "component:media-url-resolver/resolver@1.0.0",
                "component:media-url-resolver/resolver@1.0.0",
            ],
        );
        assert!(validate_component_revision(&duplicate, &V1).is_err());
    }
}

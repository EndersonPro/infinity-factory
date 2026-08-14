//! Package validation and inspection. The host treats a `.bex` as bytes only:
//! it bounds the zstd/tar archive, validates the manifest, binds the policy
//! identity to a canonical WIT digest, reuses the neutral component shape from
//! `factory-validator`, and optionally binds a catalog tuple. No runtime is
//! ever invoked.

use crate::contracts::{
    Command, Execution, HostError, HostResult, PackagePayload, Report, WitBinding,
};
use factory_validator::component_shape;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

const V1_WIT: &[u8] = include_bytes!("../../../wit/media-url-resolver/wit/media-url-resolver.wit");
const V2_WIT: &[u8] =
    include_bytes!("../../../wit/media-url-resolver-v2/wit/media-url-resolver.wit");

pub const COMPRESSED_MAX: usize = 4 * 1024 * 1024;
const EXPANDED_MAX: usize = 16 * 1024 * 1024;
const MANIFEST_MAX: usize = 64 * 1024;
const WASM_MAX: usize = 15 * 1024 * 1024;
const WIT_MAX: usize = 256 * 1024;
const ALLOWED_FIELDS: &[&str] = &[
    "manifest_version",
    "id",
    "name",
    "version",
    "type",
    "description",
    "license",
    "homepage",
    "created_at",
    "publisher",
    "keys_required",
    "abi",
    "icon",
    "remote_url",
    "thumbnail_url",
    "last_updated",
    "host_site",
    "capabilities",
    "country_allowlist",
    "resolver",
    "network_policy",
];

/// The host's own policy identity: a canonical WIT digest plus the neutral
/// import/export shape the packaged component must match. Instagram and
/// Bandcamp both bind the canonical v2 WIT digest; the host composes the
/// Bandcamp identity itself without importing any canonical Bandcamp files.
pub struct PolicyIdentity {
    pub version: &'static str,
    pub package: &'static str,
    pub abi_version: &'static str,
    pub world: &'static str,
    pub policy: Option<&'static str>,
    pub wit_digest: String,
    pub import: Option<&'static str>,
    pub export: &'static str,
}

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn canonical_v1_digest() -> String {
    hex(V1_WIT)
}

pub fn canonical_v2_digest() -> String {
    hex(V2_WIT)
}

fn valid_id(id: &str) -> bool {
    if id.is_empty() || id.len() > 128 || !id.is_ascii() {
        return false;
    }
    let mut prev_sep = true;
    for ch in id.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            prev_sep = false;
        } else if matches!(ch, '.' | '_' | '-') {
            if prev_sep {
                return false;
            }
            prev_sep = true;
        } else {
            return false;
        }
    }
    !prev_sep
}

pub fn policy_for_manifest(manifest: &Value) -> Result<PolicyIdentity, HostError> {
    if manifest["manifest_version"].as_str() != Some("1.0") {
        return Err(HostError::Package);
    }
    if manifest["type"].as_str() != Some("media-url-resolver") {
        return Err(HostError::Package);
    }
    let id = manifest["id"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or(HostError::Package)?;
    if !valid_id(id) {
        return Err(HostError::Package);
    }
    manifest["name"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or(HostError::Package)?;
    let version = manifest["version"]
        .as_str()
        .filter(|v| !v.is_empty())
        .ok_or(HostError::Package)?;
    if let Some(map) = manifest.as_object()
        && map
            .keys()
            .any(|key| !ALLOWED_FIELDS.contains(&key.as_str()))
    {
        return Err(HostError::Package);
    }
    match version {
        "1" => {
            if manifest.get("abi").is_some() {
                return Err(HostError::Package);
            }
            Ok(PolicyIdentity {
                version: "1",
                package: "component:media-url-resolver",
                abi_version: "1.0.0",
                world: "media-url-resolver",
                policy: None,
                wit_digest: canonical_v1_digest(),
                import: None,
                export: "component:media-url-resolver/resolver@1.0.0",
            })
        }
        "2" => {
            let abi = manifest["abi"].as_object().ok_or(HostError::Package)?;
            if abi["package"].as_str() != Some("component:media-url-resolver") {
                return Err(HostError::Package);
            }
            if abi["version"].as_str() != Some("2.0.0") {
                return Err(HostError::Package);
            }
            if abi["world"].as_str() != Some("media-url-resolver") {
                return Err(HostError::Package);
            }
            if abi
                .keys()
                .any(|k| !matches!(k.as_str(), "package" | "version" | "world"))
            {
                return Err(HostError::Package);
            }
            let policy = manifest["network_policy"]
                .as_str()
                .ok_or(HostError::Package)?;
            let policy_static = match policy {
                "instagram-public-v1" => "instagram-public-v1",
                "bandcamp-public-v1" => "bandcamp-public-v1",
                "facebook-public-v1" => "facebook-public-v1",
                _ => return Err(HostError::Package),
            };
            Ok(PolicyIdentity {
                version: "2",
                package: "component:media-url-resolver",
                abi_version: "2.0.0",
                world: "media-url-resolver",
                policy: Some(policy_static),
                wit_digest: canonical_v2_digest(),
                import: Some("component:media-url-resolver/https-client@2.0.0"),
                export: "component:media-url-resolver/resolver@2.0.0",
            })
        }
        _ => Err(HostError::Package),
    }
}

fn read_bounded(mut reader: impl Read, limit: usize) -> Result<Vec<u8>, HostError> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let n = reader.read(&mut chunk).map_err(|_| HostError::Package)?;
        if n == 0 {
            break;
        }
        if buf.len() + n > limit {
            return Err(HostError::Package);
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

/// Bounds compressed package bytes to at most `COMPRESSED_MAX + 1`, rejecting
/// N+1 before decode, hash, or retention.
pub fn read_compressed_bytes(reader: impl Read) -> Result<Vec<u8>, HostError> {
    let buf = read_bounded(reader.take((COMPRESSED_MAX as u64) + 1), COMPRESSED_MAX + 1)?;
    if buf.len() > COMPRESSED_MAX {
        return Err(HostError::Package);
    }
    Ok(buf)
}

struct ArchiveContents {
    manifest: Vec<u8>,
    wasm: Vec<u8>,
    wit: Vec<u8>,
}

fn extract_archive(decoded: &[u8]) -> Result<ArchiveContents, HostError> {
    let mut manifest = None;
    let mut wasm = None;
    let mut wit = None;
    let mut count = 0usize;
    let mut archive = tar::Archive::new(decoded);
    let entries = archive.entries().map_err(|_| HostError::Package)?;
    for entry in entries {
        let mut entry = entry.map_err(|_| HostError::Package)?;
        let header = entry.header();
        if !header.entry_type().is_file() {
            return Err(HostError::Package);
        }
        let name = entry
            .path()
            .map_err(|_| HostError::Package)?
            .to_string_lossy()
            .into_owned();
        if name.contains('/') || name.contains("..") || name.is_empty() {
            return Err(HostError::Package);
        }
        if header.mode().map_err(|_| HostError::Package)? != 0o644 {
            return Err(HostError::Package);
        }
        if header.uid().map_err(|_| HostError::Package)? != 0
            || header.gid().map_err(|_| HostError::Package)? != 0
            || header.mtime().map_err(|_| HostError::Package)? != 0
        {
            return Err(HostError::Package);
        }
        let size = header.size().map_err(|_| HostError::Package)? as usize;
        count += 1;
        match name.as_str() {
            "manifest.json" => {
                if size > MANIFEST_MAX {
                    return Err(HostError::Package);
                }
                let mut buf = Vec::with_capacity(size);
                entry
                    .read_to_end(&mut buf)
                    .map_err(|_| HostError::Package)?;
                manifest = Some(buf);
            }
            "plugin.wasm" => {
                if size > WASM_MAX {
                    return Err(HostError::Package);
                }
                let mut buf = Vec::with_capacity(size);
                entry
                    .read_to_end(&mut buf)
                    .map_err(|_| HostError::Package)?;
                const HEADER: [u8; 8] = [0, 97, 115, 109, 13, 0, 1, 0];
                if buf.len() < 8 || buf[..8] != HEADER {
                    return Err(HostError::Package);
                }
                wasm = Some(buf);
            }
            "plugin.wit" => {
                if size > WIT_MAX {
                    return Err(HostError::Package);
                }
                let mut buf = Vec::with_capacity(size);
                entry
                    .read_to_end(&mut buf)
                    .map_err(|_| HostError::Package)?;
                wit = Some(buf);
            }
            _ => return Err(HostError::Package),
        }
    }
    if count != 3 {
        return Err(HostError::Package);
    }
    Ok(ArchiveContents {
        manifest: manifest.ok_or(HostError::Package)?,
        wasm: wasm.ok_or(HostError::Package)?,
        wit: wit.ok_or(HostError::Package)?,
    })
}

fn validate_catalog(path: &Path, payload: &PackagePayload) -> Result<(), HostError> {
    let value: Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|_| HostError::Package)?)
            .map_err(|_| HostError::Package)?;
    if value["schema_version"].as_str() != Some("1") {
        return Err(HostError::Package);
    }
    let plugins = value["plugins"].as_array().ok_or(HostError::Package)?;
    let mut ids = std::collections::HashSet::new();
    let mut matched = 0usize;
    for plugin in plugins {
        let id = plugin["id"].as_str().ok_or(HostError::Package)?;
        if !ids.insert(id) {
            return Err(HostError::Package);
        }
        let tuple = (
            plugin["id"].as_str(),
            plugin["type"].as_str(),
            plugin["version"].as_str(),
            plugin["asset_name"].as_str(),
            plugin["asset_sha256"].as_str(),
            plugin["asset_size"].as_u64(),
            plugin["wit"]["package"].as_str(),
            plugin["wit"]["version"].as_str(),
            plugin["wit"]["world"].as_str(),
            plugin["wit"]["sha256"].as_str(),
        );
        let candidate = (
            Some(payload.id.as_str()),
            Some("media-url-resolver"),
            Some(payload.version.as_str()),
            Some(payload.asset_name.as_str()),
            Some(payload.asset_sha256.as_str()),
            Some(payload.asset_size),
            Some(payload.wit.package.as_str()),
            Some(payload.wit.version.as_str()),
            Some(payload.wit.world.as_str()),
            Some(payload.wit.sha256.as_str()),
        );
        if tuple == candidate {
            matched += 1;
        }
    }
    if matched != 1 {
        return Err(HostError::Package);
    }
    Ok(())
}

fn build(
    path: &Path,
    command: Command,
    contents: ArchiveContents,
    bytes: &[u8],
) -> Result<Report, HostError> {
    let manifest: Value =
        serde_json::from_slice(&contents.manifest).map_err(|_| HostError::Package)?;
    let identity = policy_for_manifest(&manifest)?;
    let wit_digest = hex(&contents.wit);
    if wit_digest != identity.wit_digest {
        return Err(HostError::Package);
    }
    let shape = component_shape(&contents.wasm).map_err(|_| HostError::Package)?;
    let expected_imports: Vec<String> = identity.import.into_iter().map(str::to_string).collect();
    let expected_exports: Vec<String> = vec![identity.export.to_string()];
    if shape.package_imports != expected_imports || shape.package_exports != expected_exports {
        return Err(HostError::Package);
    }
    let id = manifest["id"]
        .as_str()
        .ok_or(HostError::Package)?
        .to_string();
    let asset_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let payload = PackagePayload {
        id,
        kind: "media-url-resolver".into(),
        version: identity.version.into(),
        asset_name,
        asset_sha256: hex(bytes),
        asset_size: bytes.len() as u64,
        wit: WitBinding {
            package: identity.package.into(),
            version: identity.abi_version.into(),
            world: identity.world.into(),
            sha256: wit_digest,
        },
        network_policy: identity.policy.map(Into::into),
        inventory: vec![
            "manifest.json".into(),
            "plugin.wasm".into(),
            "plugin.wit".into(),
        ],
    };
    let result = HostResult {
        package: payload,
        unprovided_imports: shape.unprovided_imports,
        execution: Execution {
            supported: false,
            status: "deferred".into(),
        },
    };
    Ok(Report::ok(command, result))
}

fn run(command: Command, path: &Path, catalog: Option<&Path>) -> Result<Report, HostError> {
    let file = std::fs::File::open(path).map_err(|_| HostError::Package)?;
    let bytes = read_compressed_bytes(file)?;
    let decoder = zstd::Decoder::new(bytes.as_slice()).map_err(|_| HostError::Package)?;
    let decoded = read_bounded(decoder, EXPANDED_MAX + 1)?;
    if decoded.len() > EXPANDED_MAX {
        return Err(HostError::Package);
    }
    let contents = extract_archive(&decoded)?;
    let report = build(path, command, contents, &bytes)?;
    if let Some(catalog_path) = catalog
        && let Some(result) = &report.result
    {
        validate_catalog(catalog_path, &result.package)?;
    }
    Ok(report)
}

pub fn validate_package(path: &Path, catalog: Option<&Path>) -> Result<Report, HostError> {
    run(Command::Validate, path, catalog)
}

pub fn inspect_package(path: &Path, catalog: Option<&Path>) -> Result<Report, HostError> {
    run(Command::Inspect, path, catalog)
}

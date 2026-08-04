use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
use tar::{Builder, EntryType, Header};

mod revision;
pub use revision::{
    ACCEPTED, AbiShape, AcceptedRevision, ComponentShapeReport, V1, V2, accepted_for_manifest,
    accepted_shape, component_shape, validate_component_revision,
};

pub const REQUIRED_ERRORS: [&str; 8] = [
    "invalid-input",
    "unsupported-url",
    "policy-denied",
    "upstream-failure",
    "timeout",
    "malformed-response",
    "rate-limited",
    "internal",
];
pub const REQUIRED_VARIANTS: [&str; 5] = [
    "direct",
    "candidates",
    "separated",
    "unsupported",
    "deferred",
];

pub mod bounds {
    pub const URL_BYTES: usize = 2_048;
    pub const CORRELATION_ID_BYTES: usize = 64;
    pub const QUALITY_PREFERENCES: usize = 8;
    pub const CONTEXT_ENTRIES: usize = 16;
    pub const CONTEXT_KEY_BYTES: usize = 64;
    pub const CONTEXT_VALUE_BYTES: usize = 256;
    pub const COMBINED_CONTEXT_BYTES: usize = 4_096;
    pub const CANDIDATES: usize = 16;
    pub const CANDIDATE_ID_BYTES: usize = 64;
    pub const HEADERS_PER_STREAM: usize = 16;
    pub const HEADER_NAME_BYTES: usize = 64;
    pub const HEADER_VALUE_BYTES: usize = 1_024;
    pub const COMBINED_HEADER_BYTES: usize = 8_192;
    pub const FORMAT_BYTES: usize = 32;
    pub const MIME_AND_CODECS_BYTES: usize = 128;
    pub const QUALITY_LABEL_BYTES: usize = 64;
    pub const TITLE_BYTES: usize = 256;
    pub const AUTHOR_BYTES: usize = 128;
    pub const REASON_BYTES: usize = 256;
    pub const SAFE_ERROR_MESSAGE_BYTES: usize = 512;
    pub const RETRY_AFTER_SECONDS: u32 = 86_400;
}

pub fn validate_text_limit(value: &str, maximum: usize) -> Result<(), String> {
    (value.len() <= maximum)
        .then_some(())
        .ok_or_else(|| format!("value exceeds {maximum} bytes"))
}

pub fn validate_contract(wit: &str) -> Result<(), String> {
    if wit.lines().next() != Some("package component:media-url-resolver@1.0.0;") {
        return Err("invalid package identity".into());
    }
    let required = [
        "world media-url-resolver { export resolver; }",
        "resolve: func",
        "record quality-preference",
        "record context-entry",
        "record separated-streams",
        "record resolver-error",
    ];
    if let Some(missing) = required.into_iter().find(|item| !wit.contains(item)) {
        return Err(format!("missing contract identity: {missing}"));
    }
    for item in REQUIRED_ERRORS {
        if !wit.contains(item) {
            return Err(format!("missing contract member: {item}"));
        }
    }
    let variants = [
        "direct(media-stream)",
        "candidates(list<candidate>)",
        "separated(separated-streams)",
        "unsupported(unsupported)",
        "deferred(deferred)",
    ];
    for item in variants {
        if !wit.contains(item) {
            return Err(format!("missing resolution variant: {item}"));
        }
    }
    if wit.contains("import ") {
        return Err("version 1 must not import host capabilities".into());
    }
    Ok(())
}

pub fn contract_digest(wit: &str) -> String {
    format!("{:x}", Sha256::digest(wit.as_bytes()))
}

fn bounded(value: &serde_json::Value, key: &str, limit: usize) -> bool {
    value[key].as_str().is_none_or(|text| text.len() <= limit)
}

fn valid_headers(value: &serde_json::Value) -> bool {
    let Some(items) = value.as_array() else {
        return false;
    };
    let mut names: Vec<_> = items
        .iter()
        .filter_map(|item| item["name"].as_str())
        .map(str::to_ascii_lowercase)
        .collect();
    let total: usize = items
        .iter()
        .map(|item| {
            item["name"].as_str().map_or(0, str::len) + item["value"].as_str().map_or(0, str::len)
        })
        .sum();
    let original = names.len();
    names.sort_unstable();
    names.dedup();
    items.len() <= bounds::HEADERS_PER_STREAM
        && items
            .iter()
            .all(|item| item["name"].is_string() && item["value"].is_string())
        && names.len() == original
        && total <= bounds::COMBINED_HEADER_BYTES
        && items.iter().all(|item| {
            bounded(item, "name", bounds::HEADER_NAME_BYTES)
                && bounded(item, "value", bounds::HEADER_VALUE_BYTES)
        })
}

fn valid_stream(value: &serde_json::Value) -> bool {
    value["url"]
        .as_str()
        .is_some_and(|url| url.starts_with("https://") && url.len() <= bounds::URL_BYTES)
        && bounded(value, "format", bounds::FORMAT_BYTES)
        && bounded(value, "mime_type", bounds::MIME_AND_CODECS_BYTES)
        && bounded(value, "quality_label", bounds::QUALITY_LABEL_BYTES)
        && bounded(value, "codecs", bounds::MIME_AND_CODECS_BYTES)
        && valid_headers(&value["headers"])
}

pub fn validate_fixture(input: &str) -> Result<(), String> {
    let value: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let responses = value["responses"].as_object();
    let covers = |items: &serde_json::Value, required: &[&str]| {
        items.as_array().is_some_and(|values| {
            required
                .iter()
                .all(|name| values.iter().any(|value| value == name))
        })
    };
    if !responses.is_some_and(|items| {
        REQUIRED_VARIANTS
            .iter()
            .all(|name| items.contains_key(*name))
    }) || !covers(&value["errors"], &REQUIRED_ERRORS)
        || value["responses"]["separated"]["mux_plan"].is_null()
    {
        return Err("fixture coverage is incomplete".into());
    }
    let lowered = input.to_ascii_lowercase();
    let forbidden = [
        "http://",
        "webview",
        "javascript",
        "hmac",
        "ffmpeg",
        "main.api.progmore.com",
    ];
    if forbidden.iter().any(|term| lowered.contains(term)) {
        return Err("prohibited content".into());
    }
    let candidates = value["responses"]["candidates"]
        .as_array()
        .ok_or("candidates must be an array")?;
    let context = value["request"]["client_context"]
        .as_array()
        .ok_or("client_context must be an array")?;
    let quality = value["request"]["quality_preferences"]
        .as_array()
        .ok_or("quality_preferences must be an array")?;
    let mut ids: Vec<_> = candidates
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect();
    let original = ids.clone();
    ids.sort_unstable();
    ids.dedup();
    let context_bytes: usize = context
        .iter()
        .map(|item| {
            item["key"].as_str().map_or(0, str::len) + item["value"].as_str().map_or(0, str::len)
        })
        .sum();
    let mut context_keys: Vec<_> = context
        .iter()
        .filter_map(|item| item["key"].as_str())
        .collect();
    let context_key_count = context_keys.len();
    context_keys.sort_unstable();
    context_keys.dedup();
    let errors = value["error_samples"]
        .as_array()
        .ok_or("error_samples must be an array")?;
    let valid = candidates.len() <= bounds::CANDIDATES
        && ids.len() == candidates.len()
        && ids == original
        && candidates.iter().all(|item| {
            bounded(item, "id", bounds::CANDIDATE_ID_BYTES) && valid_stream(&item["stream"])
        })
        && context.len() <= bounds::CONTEXT_ENTRIES
        && context_bytes <= bounds::COMBINED_CONTEXT_BYTES
        && context_key_count == context.len()
        && context_keys.len() == context_key_count
        && context.iter().all(|item| {
            item["value"].is_string() && {
                bounded(item, "key", bounds::CONTEXT_KEY_BYTES)
                    && bounded(item, "value", bounds::CONTEXT_VALUE_BYTES)
            }
        })
        && quality.len() <= bounds::QUALITY_PREFERENCES
        && quality.iter().all(|item| {
            bounded(item, "container", bounds::FORMAT_BYTES)
                && bounded(item, "mime_type", bounds::MIME_AND_CODECS_BYTES)
                && bounded(item, "quality_label", bounds::QUALITY_LABEL_BYTES)
        })
        && valid_stream(&value["responses"]["direct"]["stream"])
        && valid_stream(&value["responses"]["separated"]["audio"])
        && valid_stream(&value["responses"]["separated"]["video"])
        && valid_headers(&value["headers"])
        && bounded(&value["request"], "source_url", bounds::URL_BYTES)
        && bounded(
            &value["request"],
            "correlation_id",
            bounds::CORRELATION_ID_BYTES,
        )
        && bounded(&value["metadata"], "title", bounds::TITLE_BYTES)
        && bounded(&value["metadata"], "author", bounds::AUTHOR_BYTES)
        && value["metadata"]["thumbnail_url"]
            .as_str()
            .is_some_and(|url| url.starts_with("https://") && url.len() <= bounds::URL_BYTES)
        && bounded(
            &value["responses"]["unsupported"],
            "reason",
            bounds::REASON_BYTES,
        )
        && bounded(
            &value["responses"]["deferred"],
            "reason",
            bounds::REASON_BYTES,
        )
        && value["responses"]["deferred"]["retry_after_seconds"]
            .as_u64()
            .is_some_and(|v| v <= u64::from(bounds::RETRY_AFTER_SECONDS))
        && errors.len() == REQUIRED_ERRORS.len()
        && errors
            .iter()
            .all(|error| bounded(error, "safe_message", bounds::SAFE_ERROR_MESSAGE_BYTES));
    valid
        .then_some(())
        .ok_or_else(|| "fixture violates field-specific ABI policy".into())
}

fn required<'a>(value: &'a Value, key: &str) -> Result<&'a str, Box<dyn Error>> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing manifest field: {key}").into())
}

pub fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Stage only tracked packages whose bytes are bound by the source catalog.
pub fn stage_release_assets(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    let catalog_bytes = fs::read(root.join("factory/bex-factory.json"))?;
    let catalog: Value = serde_json::from_slice(&catalog_bytes)?;
    if catalog["schema_version"].as_str() != Some("1") {
        return Err("unsupported factory catalog schema".into());
    }
    let plugins = catalog["plugins"]
        .as_array()
        .filter(|plugins| !plugins.is_empty())
        .ok_or("factory catalog has no plugins")?;
    let mut names = std::collections::HashSet::new();
    let mut assets = Vec::with_capacity(plugins.len());
    for plugin in plugins {
        let name = required(plugin, "asset_name")?;
        if Path::new(name).file_name().and_then(|value| value.to_str()) != Some(name)
            || !name.ends_with(".bex")
            || !names.insert(name)
        {
            return Err(format!("invalid or duplicate release asset name: {name}").into());
        }
        let fixture = root.join("fixtures/packages").join(name);
        let bytes = fs::read(&fixture)?;
        let expected_size = plugin["asset_size"]
            .as_u64()
            .ok_or_else(|| format!("missing asset_size for {name}"))?;
        let expected_sha = required(plugin, "asset_sha256")?;
        if bytes.len() as u64 != expected_size || hex(&bytes) != expected_sha {
            return Err(format!("canonical fixture does not match catalog: {name}").into());
        }
        validate_archive(&fixture)?;
        assets.push((name, bytes));
    }

    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::create_dir_all(output)?;
    for (name, bytes) in assets {
        fs::write(output.join(name), bytes)?;
    }
    fs::write(output.join("bex-factory.json"), catalog_bytes)?;
    Ok(())
}

/// Package every plugin in `root/plugins` into `output`, binding each one to
/// the exact ABI revision its manifest declares. Adding a new revision never
/// changes an unrelated plugin's packaged bytes or factory index entry:
/// every plugin is packed independently against its own accepted revision.
pub fn pack_all(root: &Path, output: &Path) -> Result<(), Box<dyn Error>> {
    if output.exists() {
        fs::remove_dir_all(output)?;
    }
    fs::create_dir_all(output)?;
    let mut entries = Vec::new();
    for item in fs::read_dir(root.join("plugins"))? {
        let plugin = item?.path();
        if !plugin.join("manifest.json").is_file() {
            continue;
        }
        let manifest: Value = serde_json::from_slice(&fs::read(plugin.join("manifest.json"))?)?;
        let id = required(&manifest, "id")?;
        let version = required(&manifest, "version")?;
        if required(&manifest, "type")? != "media-url-resolver"
            || !version.chars().all(|ch| ch.is_ascii_digit())
        {
            return Err(format!("invalid resolver manifest: {}", plugin.display()).into());
        }
        let revision = accepted_for_manifest(&manifest)
            .map_err(|error| format!("{}: {error}", plugin.display()))?;
        let crate_name = plugin
            .file_name()
            .ok_or("plugin path has no directory name")?
            .to_string_lossy()
            .replace('-', "_");
        let wasm = ["wasm32-unknown-unknown", "wasm32-wasip1", "wasm32-wasip2"]
            .into_iter()
            .map(|target| {
                root.join("target")
                    .join(target)
                    .join("release")
                    .join(format!("{crate_name}.wasm"))
            })
            .find(|path| path.is_file())
            .ok_or_else(|| format!("missing built component for {crate_name}"))?;
        let wasm_bytes = fs::read(&wasm)?;
        validate_component_revision(&wasm_bytes, revision)
            .map_err(|error| format!("{}: {error}", plugin.display()))?;
        let wit_path = root.join(revision.wit_path);
        let wit_bytes = fs::read(&wit_path)?;
        let asset_name = format!("{}.bex", id.rsplit('.').next().unwrap_or(id));
        let asset = output.join(&asset_name);
        pack(&plugin, &wasm, wit_path, &asset)?;
        validate_archive(&asset)?;
        let bytes = fs::read(&asset)?;
        entries.push(serde_json::json!({
            "id": id, "name": required(&manifest, "name")?, "version": version,
            "type": "media-url-resolver", "asset_name": asset_name,
            "asset_sha256": hex(&bytes), "asset_size": bytes.len(),
            "wit": {
                "package": revision.package, "version": revision.version,
                "world": revision.world, "sha256": hex(&wit_bytes)
            }
        }));
    }
    entries.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    if entries.is_empty() {
        return Err("no plugins were packaged".into());
    }
    let index = serde_json::json!({"schema_version":"1","plugins":entries});
    fs::write(
        output.join("bex-factory.json"),
        format!("{}\n", serde_json::to_string_pretty(&index)?),
    )?;
    Ok(())
}

fn pack(plugin: &Path, wasm: &Path, wit: PathBuf, output: &Path) -> io::Result<()> {
    let mut archive = Vec::new();
    {
        let mut tar = Builder::new(&mut archive);
        for (name, path) in [
            ("manifest.json", plugin.join("manifest.json")),
            ("plugin.wasm", wasm.to_path_buf()),
            ("plugin.wit", wit),
        ] {
            append(&mut tar, name, &fs::read(path)?)?;
        }
        tar.finish()?;
    }
    const TAR_RECORD_SIZE: usize = 10_240;
    archive.resize(archive.len().div_ceil(TAR_RECORD_SIZE) * TAR_RECORD_SIZE, 0);

    let mut encoder = zstd::Encoder::new(fs::File::create(output)?, 19)?;
    encoder.include_checksum(true)?;
    encoder.set_pledged_src_size(Some(archive.len() as u64))?;
    encoder.write_all(&archive)?;
    encoder.finish().map(|_| ())
}

fn append<W: io::Write>(tar: &mut Builder<W>, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_entry_type(EntryType::Regular);
    header.set_path(name)?;
    header.set_cksum();
    // Match GNU tar's six-digit checksum plus NUL/space encoding used by proven packages.
    let header_bytes = header.as_mut_bytes();
    header_bytes.copy_within(149..155, 148);
    header_bytes[154] = 0;
    header_bytes[155] = b' ';
    tar.append(&header, bytes)
}

pub fn validate_archive(path: &Path) -> Result<(), Box<dyn Error>> {
    let decoder = zstd::Decoder::new(fs::File::open(path)?)?;
    let mut names = Vec::new();
    for entry in tar::Archive::new(decoder).entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            return Err("non-file archive entry".into());
        }
        let name = entry.path()?.to_string_lossy().into_owned();
        if name.contains('/') || name.contains("..") {
            return Err("unsafe archive path".into());
        }
        if name == "plugin.wasm" {
            let mut header = [0_u8; 8];
            entry.read_exact(&mut header)?;
            if header != [0, 97, 115, 109, 13, 0, 1, 0] {
                return Err("plugin.wasm is not a WebAssembly component".into());
            }
        }
        names.push(name);
    }
    names.sort();
    if names != ["manifest.json", "plugin.wasm", "plugin.wit"] {
        return Err(format!("invalid archive inventory: {names:?}").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_contract_identity_and_digest() {
        let wit = include_str!("../../../wit/media-url-resolver/wit/media-url-resolver.wit");
        assert!(validate_contract(wit).is_ok());
        assert_eq!(contract_digest(wit).len(), 64);
    }

    #[test]
    fn rejects_contract_drift_and_host_imports() {
        assert!(validate_contract("package component:wrong@1.0.0;").is_err());
        let wit = include_str!("../../../wit/media-url-resolver/wit/media-url-resolver.wit");
        assert!(validate_contract(&format!("{wit}\nimport host-utils;")).is_err());
    }

    #[test]
    fn validates_all_fixture_kinds() {
        let fixture = include_str!("../../../fixtures/resolver-responses/abi-v1.json");
        assert!(validate_fixture(fixture).is_ok());
    }

    #[test]
    fn rejects_missing_fixture_variant() {
        assert!(validate_fixture(r#"{"variants":["direct"]}"#).is_err());
    }

    #[test]
    fn validates_all_boundaries_without_truncation() {
        let limits = [
            bounds::URL_BYTES,
            bounds::CORRELATION_ID_BYTES,
            bounds::QUALITY_PREFERENCES,
            bounds::CONTEXT_ENTRIES,
            bounds::CONTEXT_KEY_BYTES,
            bounds::CONTEXT_VALUE_BYTES,
            bounds::COMBINED_CONTEXT_BYTES,
            bounds::CANDIDATES,
            bounds::CANDIDATE_ID_BYTES,
            bounds::HEADERS_PER_STREAM,
            bounds::HEADER_NAME_BYTES,
            bounds::HEADER_VALUE_BYTES,
            bounds::COMBINED_HEADER_BYTES,
            bounds::FORMAT_BYTES,
            bounds::MIME_AND_CODECS_BYTES,
            bounds::QUALITY_LABEL_BYTES,
            bounds::TITLE_BYTES,
            bounds::AUTHOR_BYTES,
            bounds::REASON_BYTES,
            bounds::SAFE_ERROR_MESSAGE_BYTES,
            bounds::RETRY_AFTER_SECONDS as usize,
        ];
        for limit in limits {
            assert!(validate_text_limit(&"x".repeat(limit - 1), limit).is_ok());
            assert!(validate_text_limit(&"x".repeat(limit), limit).is_ok());
            assert!(validate_text_limit(&"x".repeat(limit + 1), limit).is_err());
        }
    }

    #[test]
    fn compatibility_bounds_and_digest_match() {
        let input = include_str!("../../../compatibility/media-url-resolver-v1.json");
        let value: serde_json::Value =
            serde_json::from_str(input).expect("test fixture must be valid");
        let b = &value["bounds"];
        let expected = [
            ("url_bytes", bounds::URL_BYTES),
            ("correlation_id_bytes", bounds::CORRELATION_ID_BYTES),
            ("quality_preferences", bounds::QUALITY_PREFERENCES),
            ("context_entries", bounds::CONTEXT_ENTRIES),
            ("context_key_bytes", bounds::CONTEXT_KEY_BYTES),
            ("context_value_bytes", bounds::CONTEXT_VALUE_BYTES),
            ("combined_context_bytes", bounds::COMBINED_CONTEXT_BYTES),
            ("candidates", bounds::CANDIDATES),
            ("candidate_id_bytes", bounds::CANDIDATE_ID_BYTES),
            ("headers_per_stream", bounds::HEADERS_PER_STREAM),
            ("header_name_bytes", bounds::HEADER_NAME_BYTES),
            ("header_value_bytes", bounds::HEADER_VALUE_BYTES),
            ("combined_header_bytes", bounds::COMBINED_HEADER_BYTES),
            ("format_bytes", bounds::FORMAT_BYTES),
            ("mime_and_codecs_bytes", bounds::MIME_AND_CODECS_BYTES),
            ("quality_label_bytes", bounds::QUALITY_LABEL_BYTES),
            ("title_bytes", bounds::TITLE_BYTES),
            ("author_bytes", bounds::AUTHOR_BYTES),
            ("reason_bytes", bounds::REASON_BYTES),
            ("safe_error_message_bytes", bounds::SAFE_ERROR_MESSAGE_BYTES),
        ];
        for (name, expected) in expected {
            assert_eq!(b[name], expected);
        }
        assert_eq!(b["retry_after_seconds"], bounds::RETRY_AFTER_SECONDS);
        let wit = include_str!("../../../wit/media-url-resolver/wit/media-url-resolver.wit");
        let digest = contract_digest(wit);
        assert_eq!(value["wit"]["sha256"], digest);
        assert!(include_str!("../../../wit/media-url-resolver/REVISION").contains(&digest));
    }

    #[test]
    fn rejects_insecure_duplicate_or_unordered_fixtures() {
        let fixture = include_str!("../../../fixtures/resolver-responses/abi-v1.json");
        assert!(validate_fixture(&fixture.replace("https://", "http://")).is_err());
        assert!(validate_fixture(&fixture.replace("Fixture", "WebView")).is_err());
        let mut value: serde_json::Value =
            serde_json::from_str(fixture).expect("test fixture must be valid");
        let items = value["responses"]["candidates"]
            .as_array_mut()
            .expect("test fixture must be valid");
        items.push(items[0].clone());
        assert!(validate_fixture(&value.to_string()).is_err());
        value["responses"]["candidates"][1]["id"] = "aaa".into();
        assert!(validate_fixture(&value.to_string()).is_err());
    }

    // Packaging/reproducibility/archive-inventory coverage lives in
    // tests/archive_mismatch.rs, which additionally binds each fixture to
    // its exact accepted ABI revision via real component-model shapes.
    #[test]
    fn rejects_unsafe_archive_inventory_with_real_component_bytes() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("test fixture must be valid")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("infinity-factory-lib-{stamp}"));
        fs::create_dir_all(&root).expect("test fixture must be valid");
        let archive = root.join("unsafe.bex");
        let encoder = zstd::Encoder::new(
            fs::File::create(&archive).expect("test fixture must be valid"),
            19,
        )
        .expect("test fixture must be valid");
        let mut tar = Builder::new(encoder.auto_finish());
        append(&mut tar, "unexpected.txt", b"bad").expect("test fixture must be valid");
        tar.finish().expect("test fixture must be valid");
        assert!(validate_archive(&archive).is_err());
        fs::remove_dir_all(root).expect("test fixture must be valid");
    }
}

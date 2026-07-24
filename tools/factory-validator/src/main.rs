use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
use tar::{Builder, Header};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("pack-all") => {
            let output = args
                .windows(2)
                .find(|w| w[0] == "--output")
                .map(|w| PathBuf::from(&w[1]))
                .unwrap_or_else(|| "dist".into());
            pack_all(Path::new("."), &output)
        }
        Some("validate") => validate_archive(Path::new(args.get(2).ok_or("missing .bex path")?)),
        _ => Err("usage: factory-validator <pack-all --output DIR|validate FILE.bex>".into()),
    }
}

fn pack_all(root: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
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
        let manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(plugin.join("manifest.json"))?)?;
        let id = required(&manifest, "id")?;
        let version = required(&manifest, "version")?;
        if required(&manifest, "type")? != "media-url-resolver"
            || !version.chars().all(|ch| ch.is_ascii_digit())
        {
            return Err(format!("invalid resolver manifest: {}", plugin.display()).into());
        }
        let crate_name = plugin
            .file_name()
            .ok_or("plugin path has no directory name")?
            .to_string_lossy()
            .replace('-', "_");
        let wasm = ["wasm32-unknown-unknown", "wasm32-wasip2"]
            .into_iter()
            .map(|target| {
                root.join("target")
                    .join(target)
                    .join("release")
                    .join(format!("{crate_name}.wasm"))
            })
            .find(|path| path.is_file())
            .ok_or_else(|| format!("missing built component for {crate_name}"))?;
        let asset_name = format!("{}.bex", id.rsplit('.').next().unwrap_or(id));
        let asset = output.join(&asset_name);
        pack(
            &plugin,
            &wasm,
            root.join("wit/media-url-resolver/wit/media-url-resolver.wit"),
            &asset,
        )?;
        validate_archive(&asset)?;
        let bytes = fs::read(&asset)?;
        let wit_bytes = fs::read(root.join("wit/media-url-resolver/wit/media-url-resolver.wit"))?;
        entries.push(serde_json::json!({
            "id": id, "name": required(&manifest, "name")?, "version": version,
            "type": "media-url-resolver", "asset_name": asset_name,
            "asset_sha256": hex(&bytes), "asset_size": bytes.len(),
            "wit": {
                "package": "component:media-url-resolver", "version": "1.0.0",
                "world": "media-url-resolver", "sha256": hex(&wit_bytes)
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

fn required<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    value[key]
        .as_str()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("missing manifest field: {key}").into())
}

fn pack(plugin: &Path, wasm: &Path, wit: PathBuf, output: &Path) -> io::Result<()> {
    let encoder = zstd::Encoder::new(fs::File::create(output)?, 19)?;
    let mut tar = Builder::new(encoder.auto_finish());
    for (name, path) in [
        ("manifest.json", plugin.join("manifest.json")),
        ("plugin.wasm", wasm.to_path_buf()),
        ("plugin.wit", wit),
    ] {
        append(&mut tar, name, &fs::read(path)?)?;
    }
    tar.finish()
}

fn append<W: io::Write>(tar: &mut Builder<W>, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut header = Header::new_gnu();
    header.set_size(bytes.len() as u64);
    header.set_mode(0o644);
    header.set_uid(0);
    header.set_gid(0);
    header.set_mtime(0);
    header.set_cksum();
    tar.append_data(&mut header, name, bytes)
}

fn validate_archive(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
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

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };
    static NEXT: AtomicU64 = AtomicU64::new(0);

    fn workspace() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("test fixture must be valid")
            .as_nanos();
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("infinity-factory-{stamp}-{unique}"));
        fs::create_dir_all(root.join("plugins/direct-url")).expect("test fixture must be valid");
        fs::create_dir_all(root.join("target/wasm32-wasip2/release"))
            .expect("test fixture must be valid");
        fs::create_dir_all(root.join("wit/media-url-resolver/wit"))
            .expect("test fixture must be valid");
        fs::write(
            root.join("plugins/direct-url/manifest.json"),
            r#"{"id":"test.direct-url","name":"Direct","version":"1","type":"media-url-resolver"}"#,
        )
        .expect("test fixture must be valid");
        fs::write(
            root.join("target/wasm32-wasip2/release/direct_url.wasm"),
            b"\0asm\r\0\x01\0",
        )
        .expect("test fixture must be valid");
        fs::write(
            root.join("wit/media-url-resolver/wit/media-url-resolver.wit"),
            b"package test:fixture;",
        )
        .expect("test fixture must be valid");
        root
    }

    #[test]
    fn packages_reproducibly_and_binds_factory_digest() {
        let root = workspace();
        let output = root.join("dist");
        pack_all(&root, &output).expect("test fixture must be valid");
        let first = fs::read(output.join("direct-url.bex")).expect("test fixture must be valid");
        pack_all(&root, &output).expect("test fixture must be valid");
        let second = fs::read(output.join("direct-url.bex")).expect("test fixture must be valid");
        assert_eq!(first, second);
        validate_archive(&output.join("direct-url.bex")).expect("test fixture must be valid");
        let bytes = fs::read(output.join("bex-factory.json")).expect("test fixture must be valid");
        let index: serde_json::Value =
            serde_json::from_slice(&bytes).expect("test fixture must be valid");
        assert_eq!(index["plugins"][0]["asset_sha256"], hex(&second));
        let wit = fs::read(root.join("wit/media-url-resolver/wit/media-url-resolver.wit"))
            .expect("test fixture must be valid");
        assert_eq!(index["plugins"][0]["wit"]["sha256"], hex(&wit));
        fs::remove_dir_all(root).expect("test fixture must be valid");
    }

    #[test]
    fn rejects_unsafe_archive_inventory() {
        let root = workspace();
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

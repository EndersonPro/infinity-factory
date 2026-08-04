use factory_validator::{pack_all, stage_release_assets, validate_archive};
use std::path::{Path, PathBuf};

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
        Some("stage-release") => {
            let output = args
                .windows(2)
                .find(|w| w[0] == "--output")
                .map(|w| PathBuf::from(&w[1]))
                .unwrap_or_else(|| "dist".into());
            stage_release_assets(Path::new("."), &output)
        }
        Some("validate") => validate_archive(Path::new(args.get(2).ok_or("missing .bex path")?)),
        _ => Err(
            "usage: factory-validator <pack-all|stage-release --output DIR|validate FILE.bex>"
                .into(),
        ),
    }
}

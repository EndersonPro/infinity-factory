#!/usr/bin/env bash
set -euo pipefail
: "${OUTPUT_DIR:=dist}"
cargo run --locked -p factory-validator -- stage-release --output "$OUTPUT_DIR"
echo "Staged canonical release assets in $OUTPUT_DIR"

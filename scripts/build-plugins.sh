#!/usr/bin/env bash
set -euo pipefail
: "${OUTPUT_DIR:=dist}"
command -v cargo-component >/dev/null || {
	echo 'cargo-component is required (0.21.1)' >&2
	exit 1
}
wasip1_lib=$(rustc --print target-libdir --target wasm32-wasip1 2>/dev/null || true)
if [[ -n "$wasip1_lib" && -d "$wasip1_lib" ]]; then
	cargo component build --release -p direct-url
elif [[ "${ALLOW_WASIP2_FALLBACK:-1}" == "1" ]]; then
	echo 'wasm32-wasip1 is unavailable; building the native WASIp2 component fallback'
	cargo build --release -p direct-url --target wasm32-wasip2
else
	echo 'wasm32-wasip1 is required for the CI component build' >&2
	exit 1
fi
rm -rf "$OUTPUT_DIR"
cargo run --locked -p factory-validator -- pack-all --output "$OUTPUT_DIR"
test -f "$OUTPUT_DIR/bex-factory.json"
count=$(find "$OUTPUT_DIR" -maxdepth 1 -type f -name '*.bex' | wc -l | tr -d ' ')
test "$count" -ge 1 || {
	echo 'No .bex package was produced' >&2
	exit 1
}
echo "Built $count plugin package(s) in $OUTPUT_DIR"

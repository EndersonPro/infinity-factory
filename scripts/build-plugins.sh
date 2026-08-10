#!/usr/bin/env bash
set -euo pipefail
: "${OUTPUT_DIR:=target/source-package-validation}"
command -v cargo-component >/dev/null || {
	echo 'cargo-component is required (0.21.1)' >&2
	exit 1
}
wasip1_lib=$(rustc --print target-libdir --target wasm32-wasip1 2>/dev/null || true)
build_plugin() {
	local crate="$1"
	if [[ -n "$wasip1_lib" && -d "$wasip1_lib" ]]; then
		cargo component build --release -p "$crate"
	elif [[ "${ALLOW_WASIP2_FALLBACK:-1}" == "1" ]]; then
		echo "wasm32-wasip1 is unavailable; building the native WASIp2 component fallback for $crate"
		cargo build --release -p "$crate" --target wasm32-wasip2
	else
		echo 'wasm32-wasip1 is required for the CI component build' >&2
		exit 1
	fi
}
build_plugin direct-url
build_plugin instagram
build_plugin bandcamp
build_plugin youtube
build_plugin x
build_plugin tiktok
rm -rf "$OUTPUT_DIR"
cargo run --locked -p factory-validator -- pack-all --output "$OUTPUT_DIR"
test -f "$OUTPUT_DIR/bex-factory.json"
count=$(find "$OUTPUT_DIR" -maxdepth 1 -type f -name '*.bex' | wc -l | tr -d ' ')
test "$count" -ge 1 || {
	echo 'No .bex package was produced' >&2
	exit 1
}
echo "Validated source build for $count plugin package(s) in $OUTPUT_DIR"

# Infinity Factory

Rust/Wasm BEX plugin factory for typed media URL resolvers.

## Repository layout

- `wit/media-url-resolver/` — canonical WIT ABI.
- `sdk/bex-media-url-resolver/` — guest SDK and export macro.
- `plugins/direct-url/` — deterministic reference plugin.
- `tools/factory-validator/` — contract and fixture validation.
- `compatibility/` and `fixtures/` — host handoff evidence.

## Prerequisites

- Rust 1.94.1 with `wasm32-unknown-unknown` for BEX output, `wasm32-wasip1` for `cargo-component`, and `wasm32-wasip2` for host compatibility work.
- `cargo-component` 0.21.1.

`bex` v0.1.3 is intentionally not used: it rejects the new `media-url-resolver` plugin type. The repository-owned validator creates compatible zstd-tar `.bex` archives.

## Validate

```bash
./scripts/check.sh
```

## Build all plugins

```bash
./scripts/build-plugins.sh
```

The command builds Wasm components, packages every plugin without skipping failures, validates each zstd-tar archive, requires at least one `.bex`, and writes `dist/bex-factory.json`.

## Runtime repository feed

Use this stable runtime registry URL:

```text
https://github.com/EndersonPro/infinity-factory/releases/latest/download/repository.json
```

It points clients to the latest release's `bex-factory.json` catalog.

## Releases

Pushing a `v*` tag triggers the release workflow. The workflow checks and builds plugins, then publishes the `.bex` packages, `bex-factory.json`, and `repository.json`. Manual runs require a non-empty `v`-prefixed tag and refuse to modify an existing release.

## Create a plugin

1. Add a crate under `plugins/<name>/` with `crate-type = ["cdylib", "rlib"]`.
2. Depend on `sdk/bex-media-url-resolver`.
3. Implement `ResolverGuest` and export it with `export_resolver!` for Wasm.
4. Add a valid integer-version `manifest.json` with type `media-url-resolver`.
5. Add deterministic native tests and run both scripts.

Do not copy private endpoints, credentials, legacy HTTP, WebView/JavaScript execution, substring host matching, or guest-side muxing.

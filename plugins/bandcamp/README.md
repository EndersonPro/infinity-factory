# Bandcamp Public Track Resolver

Resolves canonical public Bandcamp track pages (`https://{artist}.bandcamp.com/track/{slug}`) into bounded audio previews plus metadata, using exactly one anonymous host-mediated `get` and no host import beyond the existing v2 `https-client`.

## Provenance

Behavioral reference only: `yt-dlp` commit `acf8ab7a6e3024325f62426e35a17f365c4d5d54`, path `yt_dlp/extractor/bandcamp.py`. The implementation in this crate is original Rust and copies no framework code. Resolution grants **no download, purchase, redistribution, or content rights**; it must follow Bandcamp's terms and rights-holder permissions.

## What it does

- Classifies ASCII `https://{artist}.bandcamp.com/track/{slug}` strictly before any I/O; everything else returns `InvalidInput` with zero host calls.
- Issues a single headerless `get`; validates the final response stays in the page family; maps statuses to typed retryable/non-retryable errors with no leaked upstream content.
- Bounded HTML/entity decode of the single `data-tralbum` attribute, bounded JSON preflight, unique current-track selection, source-order deduplication, candidate cap 16, and corroboration-based `ts`/`token` expiry.
- Maps one retained format to `Direct`, many to ordered `Candidates` with unique bounded ids; metadata is source-derived (title/artist/thumbnail/duration-milliseconds) and never fabricated.

## Excluded

Albums, discographies, radio, embeds, private/authenticated content, free downloads/statdownload, purchases, login, cookies, browser/JS automation, HLS, DASH, DRM, media downloading, redistribution grants, and WIT/ABI changes.

## Distribution gate

The manifest declares `network_policy: "bandcamp-public-v1"`. The external native host does not yet recognize this policy, so `tools/factory-validator` intentionally rejects the manifest and the plugin is **omitted** from `factory/bex-factory.json` and `scripts/build-plugins.sh` until the external host adds support. The plugin builds and tests natively (`cargo test -p bandcamp`); packaging/registration is conditional on the host.
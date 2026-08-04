# YouTube Public Resolver

Resolves canonical public YouTube watch/shorts/short-link URLs
(`https://www.youtube.com/watch?v={id}`, `https://www.youtube.com/shorts/{id}`,
`https://youtu.be/{id}`) into playable video streams plus metadata, using the
v2 `media-url-resolver` ABI's `https-client` import: one headerless `get` for
the watch page, and — only when at least one candidate stream needs
signature-cipher decoding — a second headerless `get` for YouTube's player JS
asset.

## Provenance

Behavioral reference for the stream-resolution logic: the repo owner's own
`bloom-factory/ytvideo` crate (a BloomeeTunes `content-resolver` plugin),
`src/cipher.rs` specifically — the pure, host-independent signature-cipher
extraction/decoding algebra (three extraction strategies: modern `fQ`/`uC`
dispatch, "youtubeexplode"-equivalent split/join, and a legacy fallback) is
ported near-verbatim, since it operates on player JS text regardless of how
that text was fetched. A fourth strategy (packed constant-pool, discovered
live against a real 2026-08 player build once the other three stopped
matching current YouTube output) is original to this crate — see the module
doc comment above `extract_cipher_ops_packed_pool` in `src/cipher.rs` for how
it disambiguates the dispatcher's runtime XOR base without ever needing a
literal variable name.

Note: this WASM plugin's `get`-only ABI means it is stuck on the
signature-cipher path (YouTube's own "last resort" tier, behind ANDROID_VR/
IOS InnerTube client identities that return already-resolved URLs). The
`infinity` app consumes this plugin only as a fallback — its primary YouTube
path is native, non-sandboxed Rust (`infinity/rust/src/youtube_native.rs`)
using those same InnerTube identities, which this ABI has no way to reach
(no generic POST). The `n`-parameter CDN throttling transform is also not
decoded here, same gap `bloom-factory/ytvideo` has; the app's native path
sidesteps it entirely rather than solving it.

Not ported: `bloom-factory/ytvideo`'s `client.rs`/`parser.rs`/`mapper.rs`,
which implement a much larger `content-resolver` surface (home sections,
search, albums, artists, playlists, radio) and fetch streams via YouTube's
InnerTube `player` POST endpoint using several device-client identities
(ANDROID_VR/IOS/TVHTML5). This v2 `media-url-resolver` ABI exposes only `get`
(host/path-allowlisted) and a GraphQL-shaped `post-public-graphql` fixed to
Instagram's endpoint — no generic POST is available — so the InnerTube
client strategy cannot be reused. This plugin instead extracts
`ytInitialPlayerResponse` from the watch page HTML directly (the classic
`yt-dlp`-style approach), which only needs `get`.

## What it does

- Classifies ASCII `watch`/`shorts`/`youtu.be` URLs strictly before any I/O,
  canonicalizing every accepted input to `https://www.youtube.com/watch?v={id}`
  before issuing any request.
- Extracts `ytInitialPlayerResponse` from a unique, balanced-brace-scanned
  `<script>` block; maps `playabilityStatus` to typed retryable/non-retryable
  errors without leaking upstream text into `safe_message`.
- Prefers progressive (muxed) `formats` when present (`Direct`/`Candidates`);
  otherwise falls back to the best adaptive audio + best adaptive video
  (`Separated`), since YouTube has not offered high-resolution progressive
  streams in years.
- Decodes `signatureCipher`/`cipher` blobs only when a selected format needs
  it, fetching the player JS referenced by the watch page's `jsUrl` field.
  Picks a mux container compatible with both streams without a re-encode:
  same-family pairs keep their native container (mp4+mp4, webm+webm); a
  mixed pair uses Matroska.

## Known limitations

- No caching between calls (this ABI exposes no persistent storage
  capability), so every `resolve` re-extracts cipher ops from a freshly
  fetched player JS when needed.
- YouTube's separate "n" parameter throttling transform is not decoded —
  carried over unchanged from `bloom-factory/ytvideo`, which has the same
  gap. Streams remain playable but may be CDN-throttled.
- `resolve-request.quality-preferences` is not consulted (matches the
  Instagram/Bandcamp v2 plugins' precedent, which also ignore it): the best
  available progressive/adaptive formats by height/bitrate are always
  selected.
- Only `?v={id}` is accepted on `/watch`; other query parameters
  (`list`, `t`, `si`, ...) are rejected rather than tolerated, mirroring
  Bandcamp's zero-tolerance precedent over Instagram's allowlisted-param one.

## Excluded

Playlists, channels, search, comments, captions/subtitles, live streams,
age/region-gated content, DRM'd formats, cookies/login, browser/JS
automation, DASH manifest parsing, media downloading, redistribution grants,
and WIT/ABI changes.

## Distribution

The manifest declares `network_policy: "youtube-public-v1"`, accepted by
`tools/factory-validator`'s ABI v2 revision table (`tools/factory-validator/
src/revision.rs`, the `V2` static's `network_policies`). The plugin is built
by `scripts/build-plugins.sh` and staged as a proven release asset in
`factory/bex-factory.json`/`fixtures/packages/youtube.bex` — verified against
a real device and live YouTube (real page fetch, real cipher decode, a real
signed CDN URL back), same evidence bar `plugins/bandcamp` was held to.
Whether that URL actually downloads depends on the unresolved `n`-parameter
throttling gap noted above; the app's fallback ordering means this plugin is
only reached once the native path has already failed, so that gap matters
less in practice than it would as a primary path.

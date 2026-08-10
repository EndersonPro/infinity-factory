# TikTok Public Video Resolver v2 Host Contract

This repository publishes the contract; the **external native host** implements and enforces it. The external host repository does not yet recognize `tiktok-public-v1`, so this plugin is not registered in `factory/bex-factory.json` and is not built into a `.bex` by `scripts/build-plugins.sh` until the host adds support and the `bex` packaging CLI is available in a provisioned environment.

## Identity

- Package: `component:media-url-resolver@2.0.0`
- World/export: `media-url-resolver` / `resolver`
- Sole import: `https-client`
- Operation: `get` only (no `post-public-graphql`)
- Policy: `compatibility/media-url-resolver-v2-tiktok-policy.json`
- Network policy id: `tiktok-public-v1`

The WIT bytes and the v2 ABI are unchanged; this contract reuses the existing `media-url-resolver-v2` WIT exactly.

## Network authority

`get` accepts only canonical ASCII `https://www.tiktok.com/@{user}/video/{id}` URLs: one `@`-prefixed user segment (1-64 bytes from `[A-Za-z0-9._-]`, the literal `_` admitted as a normal user, not a sentinel; mirrors yt-dlp's `_create_url` fallback at `yt_dlp/extractor/tiktok.py:106-108`) and one video id (1-19 ASCII digits), with no query string, fragment, userinfo, or explicit port. It rejects `vm.tiktok.com`, `vt.tiktok.com`, `www.tiktok.com/t/`, profile-only, `@user/live`, `m.tiktok.com`, `webcast.tiktok.com`, `www.douyin.com`, the bare apex `tiktok.com`, and any redirect that leaves the canonical page family. The GET carries only `accept` and `accept-language`; every other guest-requested header name is rejected by the SDK authority gate before any host I/O. No cookies, no JS, no POST, no TLS fingerprint shaping.

## Universal-data extraction

The resolver scans the response body for the single `<script id="__UNIVERSAL_DATA_FOR_REHYDRATION__" type="application/json">{...}</script>` block and JSON-parses it; it never executes JavaScript. It binds `__DEFAULT_SCOPE__.webapp.video-detail.itemInfo.itemStruct.video` (NOT the empty modern `itemModule` field). The committed `tt_v3.html` probe places `statusCode` on `webapp.video-detail` directly; a non-zero `statusCode` (observed `10204` variants `status_self_see`, `person_geo_fencing`, item-does-not-exist, and any defensive non-zero value) maps to `Unsupported`, never to a download failure.

## Media URL resolution

Candidate HTTPS MP4 URLs are collected from `playAddr`, the optional `downloadAddr`, `PlayAddrStruct.UrlList`, and `bitrateInfo[*].PlayAddr.UrlList`, in source order. The output is filtered to URLs on the admitted CDN families — `host.ends_with(".tiktokcdn.com")` (non-apex), `host.ends_with(".tiktokv.com")` (non-apex), or `host` starting with `v` and ending with `-webapp-prime.tiktok.com` (the no-`v` apex `webapp-prime.tiktok.com` is rejected) — all HTTPS, no userinfo, no port, no fragment, and ≤2048-byte URL strings. Every `www.tiktok.com/aweme/v1/play/...` gateway URL is rejected. Candidates are deduplicated by URL value preserving source order and capped at 16 (the SDK bound). One survivor → `Resolution::Direct`; ≥2 → `Resolution::Candidates`; zero → `Resolution::Unsupported`.

## Download-disabled public video

When `author.downloadSetting != 0`, TikTok hides the in-app Save button but `playAddr` still serves a direct MP4; the resolver resolves such public videos through `playAddr` even when `downloadAddr` is absent. Absence of `downloadAddr` alone never produces `Unsupported` when `playAddr` is present.

## Sensitive state

Cookies, authorization, `statusMsg`, the universal-data JSON body, and raw transport diagnostics are never logged or surfaced. The guest-visible response carries only bounded CDN MP4 candidate URLs (format `mp4`, MIME `video/mp4`) with no expiry, byte-range, or header claims. No headers are emitted on the media streams.

## Excluded workflows

Short/share links (`vm.`, `vt.`, `www.tiktok.com/t/`), live (`webcast.tiktok.com`, `@user/live`), slideshows (audio-only), profile and home pages, the application API (`api16-…tiktokv.com/aweme/v1/...`), Douyin, JS challenges, browser automation, HLS, DASH, DRM, media downloading, and any download/redistribution rights are out of scope and rejected without additional host operations.

## Integration boundary

Conformance requires the committed `tt_v3.html` happy-path probe plus synthetic `10204`, no-universal-block, empty-video, and malformed-JSON vectors, exact component import/export validation, and the validator allowlisting `tiktok-public-v1`. This repository does not claim to provide the native host implementation; registration is gated on external `tiktok-public-v1` support in `add-tiktok-runtime-host` and on a provisioned `.bex` packaging step.
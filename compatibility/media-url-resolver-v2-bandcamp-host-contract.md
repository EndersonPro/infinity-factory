# Bandcamp Public Track Resolver v2 Host Contract

This repository publishes the contract; the **external native host** implements and enforces it. The external host repository does not yet recognize `bandcamp-public-v1`, so this plugin is not registered in `factory/bex-factory.json` and is not built by `scripts/build-plugins.sh` until the host adds support.

## Identity

- Package: `component:media-url-resolver@2.0.0`
- World/export: `media-url-resolver` / `resolver`
- Sole import: `https-client`
- Operation: `get` only (no `post-public-graphql`)
- Policy: `compatibility/media-url-resolver-v2-bandcamp-policy.json`
- Network policy id: `bandcamp-public-v1`

The WIT bytes and the v2 ABI are unchanged; this contract reuses the existing `media-url-resolver-v2` WIT exactly.

## Network authority

`get` accepts only canonical ASCII `https://{artist}.bandcamp.com/track/{slug}` URLs: one lowercase RFC DNS-label artist (1-63 bytes, no leading/trailing hyphen) and one non-empty lowercase alphanumeric/hyphen slug (1-128 bytes). It rejects userinfo, explicit ports, query strings, fragments, credentials, extra path segments, `www.bandcamp.com`, the apex `bandcamp.com`, non-ASCII, and any redirect that leaves the `*.bandcamp.com/track/{slug}` page family. The GET carries no headers.

Media URLs are validated separately through the constrained v2 safe-HTTPS check (HTTPS, host present, no userinfo, no port, no fragment) and currently resolve under `*.bcbits.com`.

## Track selection and expiry

The resolver selects the unique track whose `id` matches `current.id`, deduplicates `file` URLs in source order, caps to 16 candidates, and discards empty or expired entries. A stream expiry is the decimal `ts` query value only when a `token` query value corroborates it by prefix. `ts` absent means no expiry; a `ts` without a corroborating, matching `token` (or a duplicate/non-decimal `ts`) is a conflicting marker and fails closed as `MalformedResponse`.

## Sensitive state

Cookies, authorization, LSD-equivalent tokens, `trackinfo` bodies, media URLs, and raw transport diagnostics are never logged or surfaced. The guest-visible response carries only bounded source-derived format metadata, expiry, recognized MIME/codec, a safe thumbnail, and selected-track title/artist/duration. No headers or byte-range claims are emitted.

## Excluded workflows

Albums, discographies, radio, embeds, private/authenticated content, free downloads, purchases, login, cookies, browser automation, HLS, DASH, DRM, media downloading, and any download/redistribution rights are out of scope and rejected without additional host operations.

## Integration boundary

Conformance requires positive and negative page/media vectors plus exact component import/export validation. This repository does not claim to provide the native host implementation; registration is gated on external `bandcamp-public-v1` support.
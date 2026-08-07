# X Syndication Resolver v2 Host Contract

This repository publishes the contract; the **external native host** implements and enforces it. The external host does not yet recognize `x-syndication-v1`, so no plugin is registered in `factory/bex-factory.json` and none is built by `scripts/build-plugins.sh` until it does. This document covers transport admission only — the resolver crate is a later delivery.

## Identity

- Package: `component:media-url-resolver@2.0.0`
- World/export: `media-url-resolver` / `resolver`
- Sole import: `https-client`
- Operation: `get` only (no `post-public-graphql`)
- Policy: `compatibility/media-url-resolver-v2-x-policy.json`
- Network policy id: `x-syndication-v1`

The WIT bytes and the v2 ABI are unchanged; this contract reuses the existing `media-url-resolver-v2` WIT exactly, and `wit/media-url-resolver-v2/REVISION` is untouched.

`post-public-graphql` is excluded from `operations` rather than declared and left unused. X's own GraphQL API is not reachable through it — that operation is pinned to Meta's endpoint and form fields — and a policy claiming an operation the resolver cannot use would be a wider grant than the work needs.

## Network authority

`get` accepts exactly one URL shape: `https://cdn.syndication.twimg.com/tweet-result?id={id}&token={token}`.

The authority is compared as an exact literal. No suffix form, no wildcard, no parent domain: `syndication.twimg.com`, `twimg.com`, `cdn-syndication.twimg.com`, `evil.cdn.syndication.twimg.com`, and `cdn.syndication.twimg.com.evil.tld` are all different hosts and all refused. `compatibility/v2/abi-identity-vectors.json` already names `wildcard-host` and `deceptive-host` as invalid ABI shapes and this policy does not become the first exception.

The path is exactly `/tweet-result` — two segments, the first empty. The URL parser resolves `..` before the authority branch inspects the path, so `/../tweet-result` normalises to the canonical path and is admitted; what would go on the wire is byte-identical to a canonical request. `/tweet-result/../../evil` normalises to `/evil` and is refused.

The query is a closed vocabulary in fixed order: `id` then `token`, exactly two pairs. `id` is 1-19 ASCII digits with no leading zero, bounded so every admitted value parses into a `u64`. `token` is 1-16 characters from `1-9a-z`; `0` is excluded because the derivation strips it, so a token carrying one is a string the guest cannot produce. A missing, extra, repeated, reordered, empty, or out-of-class value is refused.

Userinfo, explicit ports, fragments, non-HTTPS schemes, and case or normalisation drift in the authority are refused by the shared parser for every authority alike.

The GET carries no headers. The guest may supply only `accept` and `accept-language` under the v2 contract and this policy adds no exception: the `User-Agent` remains host-owned. The endpoint was measured to answer identically under `Googlebot`, a desktop browser agent, and no override, so the reference implementation's `User-Agent: Googlebot` is vestigial.

Media URLs are validated separately through the constrained v2 safe-HTTPS check and are expected under `video.twimg.com`.

## Sensitive state

Cookies, authorization headers, guest tokens, bearer tokens, response bodies, and raw transport diagnostics are never logged or surfaced. The guest never receives or constructs an authenticated credential; the API path that requires one is excluded below.

## Excluded workflows

The GraphQL API, guest-token minting, bearer constants, login, protected accounts, quoted-tweet recursion, threads, Spaces, broadcasts, image extraction, embeds, private content, cookies, browser automation, HLS, DASH, DRM, media downloading, and any download or redistribution rights are out of scope and rejected without additional host operations.

## Service terms

Source provenance and service permission are separate questions. The reference implementation is public-domain licensed, which says nothing about the operator's terms of service, and X's prohibit automated collection. The syndication endpoint is undocumented and may be withdrawn or begin validating the `token` without notice; the resolver derives the real token so the request shape stays valid if that happens. This exposure is recorded here for the integrating product to decide on, and is not resolved by this contract.

## Integration boundary

Conformance requires the positive and negative URL vectors in `sdk/bex-media-url-resolver-v2/tests/x_syndication_admission.rs` plus exact component import/export validation. This repository does not provide the native host implementation; registration is gated on external `x-syndication-v1` support.

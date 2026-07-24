# Resolver ABI v2 External Host Contract

This repository publishes the contract; the **external native host** implements and enforces it.

## Identity

- Package: `component:media-url-resolver@2.0.0`
- World/export: `media-url-resolver` / `resolver`
- Sole import: `https-client`
- Operations: `get`, `post-public-graphql`
- Policy: `compatibility/media-url-resolver-v2-policy.json`

Hosts MUST bind exact WIT bytes and SHA-256. Unknown, partial, or mixed revisions fail closed. ABI v1 remains unchanged.

## Network authority

`get` accepts only canonical HTTPS content paths on exact hosts `instagram.com` and `www.instagram.com`. It rejects userinfo, IP literals, fragments, explicit ports, malformed authorities, unknown paths, headers outside `accept` and `accept-language`, and any redirect leaving the same policy.

`post-public-graphql` is not generic POST. It accepts only `https://www.instagram.com/api/graphql`, rejects redirects, validates bounded LSD/friendly-name/doc-ID/variables fields, requires the configured friendly-name/doc-ID pair, and constructs the exact form, content type, static headers, and content length itself.

The host enforces every finite value in the policy JSON, including one 10-second deadline over DNS through complete body, at most three GET redirects, and a 4 MiB response body with no partial overflow result.

## Sensitive state

Cookies are host-owned, opaque, package/instance/policy scoped, finite, anonymous, and ephemeral. `Cookie` and `Set-Cookie` never cross WIT. Authenticated account sessions are prohibited.

LSD comes from public page state but is sensitive ephemeral input. Hosts MUST NOT log or expose LSD, variables, form bodies, cookies, response bodies, query values, credentials, or raw transport diagnostics. Guest-visible headers are exactly the policy allowlist.

## Error mapping

Hosts return only the typed `https-error` variants. Invalid host-produced status, URL, headers, or body map to `malformed-upstream`. Limits reject without truncation. Errors contain no dynamic upstream text.

## Integration boundary

Conformance requires positive and negative policy vectors plus exact component import/export validation. This repository does not claim to provide the Flutter or native host implementation.

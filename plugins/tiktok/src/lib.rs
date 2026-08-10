mod mapping;
mod payload;
mod retrieval;
mod url;

// `error` helpers are exercised by retrieval (invalid_input, transport,
// malformed, status and the private/unavailable arms status reaches) and by
// payload (unsupported for non-zero statusCode and missing/malformed video).
mod error;

pub use mapping::{is_gateway_url, is_safe_tiktok_output_url, map};
pub use payload::{VideoData, parse_universal_data};
pub use retrieval::{resolve_public, resolve_public_at, retrieve_https, retrieve_source};
pub use url::{CanonicalUrl, classify_url};

use bex_media_url_resolver_v2::{ResolveResponse, ResolverError};

/// Compose the pure, hermetic pipeline (universal-data parse -> map). No host
/// interaction; used by tests and by `resolve_public_at`, which injects the
/// live classify + GET. TikTok carries no `now`-dependent stream expiry
/// (unlike bandcamp's `ts`/`token` audio URLs), so no clock is injected.
pub fn parse_and_map(body: &[u8]) -> Result<ResolveResponse, ResolverError> {
    let video = payload::parse_universal_data(body)?;
    Ok(ResolveResponse {
        metadata: None,
        resolution: mapping::map(&video),
    })
}

/// Exported v2 resolver component. `classify_url` enforces the TikTok source
/// contract before any host call; the in-process pipeline issues exactly one
/// anonymous `get`; the assembled response is validated before it leaves the
/// guest. The v2 WIT bytes and host ABI are unchanged.
#[cfg(target_arch = "wasm32")]
pub struct Component;

#[cfg(target_arch = "wasm32")]
impl bex_media_url_resolver_v2::ResolverGuest for Component {
    fn resolve(
        request: bex_media_url_resolver_v2::ResolveRequest,
    ) -> Result<bex_media_url_resolver_v2::ResolveResponse, bex_media_url_resolver_v2::ResolverError>
    {
        let mut client = bex_media_url_resolver_v2::WasmHttpsClient;
        let response = resolve_public(&mut client, &request.source_url)?;
        bex_media_url_resolver_v2::validate_resolver_response(&response)?;
        Ok(response)
    }
}

#[cfg(target_arch = "wasm32")]
bex_media_url_resolver_v2::export_resolver_v2!(Component);

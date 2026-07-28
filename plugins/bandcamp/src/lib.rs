mod error;
mod html;
mod mapping;
mod payload;
mod retrieval;
mod url;

pub use retrieval::{resolve_public, resolve_public_at};
pub use url::{CanonicalUrl, classify_url};

use bex_media_url_resolver_v2::{ResolveResponse, ResolverError};

/// Compose the pure, hermetic page pipeline (HTML extraction -> bounded
/// payload selection at `now` -> result mapping) with no host interaction.
/// Used by tests and by `resolve_public_at`, which injects the live GET.
pub fn parse_and_map(body: &[u8], now: u64) -> Result<ResolveResponse, ResolverError> {
    let data = html::extract(body).map_err(|_| error::malformed())?;
    let selection = payload::parse(&data.json, now, data.thumbnail.as_deref())?;
    Ok(mapping::map(selection))
}

/// Exported v2 resolver component. `classify_url` enforces the Bandcamp source
/// contract before any host call; the in-process page pipeline issues exactly
/// one anonymous `get`; the assembled response is validated before it leaves the
/// guest. The v2 WIT bytes and host ABI are unchanged (no input rewriter, no
/// query tolerance, no headers, no other host operations).
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

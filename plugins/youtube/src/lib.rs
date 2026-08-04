mod cipher;
mod error;
mod mapping;
mod page;
mod payload;
mod retrieval;
mod url;

pub use retrieval::{parse_and_map, resolve_public};
pub use url::{VideoId, classify_url};

/// Wasm component export glue. `resolve_public` already composes
/// `classify_url` -> watch-page GET -> (conditional) player-JS GET ->
/// signature-cipher decode -> mapping into the exact
/// `Result<ResolveResponse, ResolverError>` shape the ABI v2 `Guest` trait
/// requires, so no additional request rewriting is needed here. Response
/// validation mirrors the Instagram/Bandcamp v2 plugins for parity.
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

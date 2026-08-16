mod error;
mod html;
mod mapping;
mod page;
mod payload;
mod retrieval;
mod url;

pub use html::{extract_data_sjs, is_login_wall};
pub use mapping::{map_progressive, unsupported_response};
pub use page::extract_tahoe_tokens;
pub use payload::{Media, extract_media, parse_tahoe_response};
pub use retrieval::resolve_public;
pub use url::{CanonicalUrl, classify_url};

/// Wasm component export glue. `resolve_public` already composes
/// `classify_url` -> single GET -> `data-sjs`/Tahoe mapping into the exact
/// `Result<ResolveResponse, ResolverError>` shape the ABI v2 `Guest` trait
/// requires, so no additional request rewriting is needed here.
///
/// `WasmHttpsClient` admits each canonical Facebook GET through the shared SDK
/// policy before host I/O. The plugin classifier still validates its input
/// grammar independently, and response safety remains gated at the end via
/// `validate_resolver_response`.
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

mod error;
mod fallback;
mod mapping;
mod page;
mod payload;
mod retrieval;
mod url;

pub use fallback::resolve_public;
pub use page::{PageState, extract_page_state};
pub use retrieval::{RetrievalResult, retrieve_public, validate_retrieval_config};
pub use url::{CanonicalUrl, classify_url};

/// Wasm component export glue. `resolve_public` already composes
/// `classify_url` -> `retrieve_public` -> GraphQL/Relay mapping into the
/// exact `Result<ResolveResponse, ResolverError>` shape the ABI v2 `Guest`
/// trait requires, so no additional request rewriting is needed here.
///
/// Deliberately NOT called here: `bex_media_url_resolver_v2::validate_resolver_request`.
/// That validator enforces `valid_get_url`, which rejects any `source_url`
/// carrying a query string. `classify_url` (see `url.rs`) intentionally
/// accepts and discards `igsh`/`utm_source`/`utm_medium` query keys per
/// design.md, matching real Instagram share links. Calling the generic
/// request validator first would silently make that documented, tested
/// query-acceptance behavior unreachable through the real exported `resolve`
/// entrypoint. This is a design gap between WU2C's generic bounds validator
/// and WU4's query-tolerant classifier; response validation has no such
/// conflict and is applied below for parity with the v1 template.
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

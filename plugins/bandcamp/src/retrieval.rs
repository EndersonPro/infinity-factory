use crate::{classify_url, error, parse_and_map};
use bex_media_url_resolver_v2::{
    GetRequest, HttpsClient, ResolveResponse, ResolverError, validate_https_response,
};

/// Resolve `source` against `client` at an injected `now` (unix seconds).
/// Issues exactly one headerless GET on accepted inputs; the page pipeline
/// drops expired audio at `now`. Test seams inject `now` to avoid the host clock.
pub fn resolve_public_at(
    client: &mut impl HttpsClient,
    source: &str,
    now: u64,
) -> Result<ResolveResponse, ResolverError> {
    let canonical = classify_url(source).map_err(|_| error::invalid_input())?;
    let page = client
        .get(GetRequest {
            url: canonical.as_str().into(),
            headers: vec![],
        })
        .map_err(error::transport)?;
    validate_https_response(&page).map_err(|_| error::malformed())?;
    error::status(page.status)?;
    parse_and_map(&page.body, now)
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

/// Production entry used by the exported component: supplies the system clock.
pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    resolve_public_at(client, source, now_unix())
}

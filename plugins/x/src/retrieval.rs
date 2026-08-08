use crate::{classify_url, error, parse_and_map, token::syndication_token};
use bex_media_url_resolver_v2::{
    GetRequest, HttpsClient, ResolveResponse, ResolverError, validate_https_response,
};

/// The one endpoint this resolver talks to.
const ENDPOINT: &str = "https://cdn.syndication.twimg.com/tweet-result";

/// The request URL for an accepted post id.
///
/// Built here from the classified id rather than from the input, so nothing a
/// share sheet appended to the pasted link can reach the network.
pub fn request_url(post_id: &crate::url::PostId) -> String {
    format!(
        "{ENDPOINT}?id={}&token={}",
        post_id.as_str(),
        syndication_token(post_id.as_u64())
    )
}

/// Resolve `source` against `client`.
///
/// Exactly one headerless GET on accepted input. The `User-Agent` is the
/// host's: the endpoint was measured to answer identically with and without an
/// override, so the reference implementation's `Googlebot` buys nothing and
/// the guest asks for no header exception.
pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    let post_id = classify_url(source).map_err(|_| error::invalid_input())?;
    let response = client
        .get(GetRequest {
            url: request_url(&post_id),
            headers: vec![],
        })
        .map_err(error::transport)?;
    validate_https_response(&response).map_err(|_| error::malformed())?;
    error::status(response.status)?;
    parse_and_map(&response.body)
}

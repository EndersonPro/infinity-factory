use crate::url::CanonicalUrl;
use crate::{classify_url, error};
use bex_media_url_resolver_v2::{
    GetRequest, Header, HttpsClient, HttpsResponse, ResolveResponse, ResolverError,
    validate_https_response,
};

/// The exact two request headers a TikTok canonical-page GET carries. The
/// v2 SDK authority gate admits ONLY `accept` and `accept-language` for guest
/// GET requests (`sdk/bex-media-url-resolver-v2/src/lib.rs:320-322`); every
/// other guest-requested header name is rejected before any host I/O
/// (spec Req 2 scenarios "Rejects Authorization/Cookie/Referer header").
const ACCEPT: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

fn request_headers() -> Vec<Header> {
    vec![
        Header {
            name: "accept".into(),
            value: ACCEPT.into(),
        },
        Header {
            name: "accept-language".into(),
            value: ACCEPT_LANGUAGE.into(),
        },
    ]
}

/// Spec Req 2: issue exactly ONE host-mediated HTTPS GET to a pre-classified
/// canonical TikTok URL carrying only `accept` + `accept-language` headers,
/// then validate the response through the v2 SDK gates and map HTTP status.
/// No cookies, no JS, no POST, no TLS shaping. The body is returned raw for
/// the payload layer; transport and response errors map to typed
/// `ResolverError` (`error::transport` / `error::malformed` / `error::status`).
pub fn retrieve_https(
    client: &mut impl HttpsClient,
    url: &CanonicalUrl,
) -> Result<HttpsResponse, ResolverError> {
    let response = client
        .get(GetRequest {
            url: url.as_str().into(),
            headers: request_headers(),
        })
        .map_err(error::transport)?;
    validate_https_response(&response).map_err(|_| error::malformed())?;
    error::status(response.status)?;
    Ok(response)
}

/// Spec Req 2 transport seam: classify the source URL (zero host calls on
/// rejection → InvalidInput) then issue the single bounded GET. Returns the
/// raw validated response without parsing.
pub fn retrieve_source(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<HttpsResponse, ResolverError> {
    let canonical = classify_url(source).map_err(|_| error::invalid_input())?;
    retrieve_https(client, &canonical)
}

/// Compose classify -> retrieve -> parse -> map (design.md data flow). The
/// hermetic seam used by tests; the wasm component supplies the live client.
pub fn resolve_public_at(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    let response = retrieve_source(client, source)?;
    crate::parse_and_map(&response.body)
}

/// Production entry used by the exported component (design.md:101-105). The
/// tiktok pipeline carries no `now`-dependent expiry (unlike bandcamp's
/// `ts`/`token` audio streams), so the host clock is not injected here.
pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    resolve_public_at(client, source)
}

use crate::{
    CanonicalUrl, Media, classify_url, error, extract_data_sjs, extract_media,
    extract_tahoe_tokens, is_login_wall, map_progressive, parse_tahoe_response,
    unsupported_response,
};
use bex_media_url_resolver_v2::{
    GetRequest, Header, HttpsClient, HttpsResponse, ResolveResponse, ResolverError,
    build_tahoe_request,
};

/// The exact two request headers a Facebook canonical-page GET carries. The v2
/// SDK authority gate admits only `accept` and `accept-language` for guest GET
/// requests; every other guest-requested header name is rejected before any
/// host I/O (spec Req 2 "Sends only accept and accept-language headers"). The
/// host owns the Chrome desktop User-Agent under `HostPolicy::FacebookPublicV1`
/// (`exploration.md:25-28`); the guest MUST NOT set it.
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

/// Issue exactly one host-mediated HTTPS GET to the canonical URL carrying only
/// `accept` + `accept-language` headers, then map HTTP status (spec Req 2).
/// `validate_https_response` is intentionally NOT called: its `final_url` gate
/// reuses `valid_get_url`, whose authority allow-list does not yet include
/// `www.facebook.com` (shipped with the `HostPolicy::FacebookPublicV1` host
/// grant); the generic gate would reject every Facebook GET. The plugin
/// classifies the URL itself and trusts the host to bound the response.
fn retrieve_https(
    client: &mut impl HttpsClient,
    url: &CanonicalUrl,
) -> Result<HttpsResponse, ResolverError> {
    let response = client
        .get(GetRequest {
            url: url.as_str().into(),
            headers: request_headers(),
        })
        .map_err(error::transport)?;
    error::status(response.status)?;
    Ok(response)
}

/// Compose `classify_url -> one GET -> data-sjs -> progressive URL` and, when no
/// progressive URL survives, a single `post-tahoe` fallback (spec Req 1-7).
/// Login walls map to `private-or-unavailable`; missing/DASH/HLS-only media or
/// absent Tahoe tokens map to `Unsupported`; malformed upstream JSON maps to
/// `malformed-response`. Exactly one GET and at most one Tahoe POST are issued.
pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    let canonical = classify_url(source).map_err(|_| error::invalid_input())?;
    let response = retrieve_https(client, &canonical)?;
    let body = std::str::from_utf8(&response.body).map_err(|_| error::malformed())?;
    if is_login_wall(body) {
        return Err(error::private());
    }
    let blocks = extract_data_sjs(body);
    match extract_media(&blocks)? {
        None => Ok(unsupported_response(
            "public Facebook page has no video media object",
        )),
        Some(media) if !media.progressive.is_empty() => Ok(map_progressive(&media)),
        Some(media) if media.has_dash_hls => Ok(unsupported_response(
            "public Facebook media is DASH or HLS only",
        )),
        Some(_) => {
            let Some(tokens) = extract_tahoe_tokens(body) else {
                return Ok(unsupported_response(
                    "public Facebook page lacks Tahoe tokens",
                ));
            };
            let tahoe_url = format!(
                "https://www.facebook.com/video/tahoe/async/{}/",
                canonical.video_id()
            );
            let call = match build_tahoe_request(
                &tahoe_url,
                tokens.fb_dtsg,
                &tokens.pkg_cohort,
                &tokens.client_rev,
            ) {
                Ok(call) => call,
                Err(_) => {
                    return Ok(unsupported_response(
                        "public Facebook Tahoe request rejected by policy",
                    ));
                }
            };
            let tahoe = client.post_tahoe(call).map_err(error::transport)?;
            error::status(tahoe.status)?;
            let tahoe_urls = parse_tahoe_response(&tahoe.body)?;
            if tahoe_urls.is_empty() {
                return Ok(unsupported_response(
                    "public Facebook Tahoe response has no media URLs",
                ));
            }
            Ok(map_progressive(&Media {
                progressive: tahoe_urls,
                has_dash_hls: false,
                title: None,
                author: None,
                thumbnail: None,
                duration_milliseconds: None,
            }))
        }
    }
}

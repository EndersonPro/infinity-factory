use crate::{classify_url, error, mapping, page, payload};
use bex_media_url_resolver_v2::{
    GetRequest, HttpsClient, ResolveResponse, ResolverError, validate_https_response,
};

/// Pure, hermetic core: given an already-fetched watch page body and
/// (conditionally) an already-fetched player JS body, parse, decode, and map
/// to the final `ResolveResponse`. No host interaction here — used directly
/// by tests and internally by `resolve_public`, which supplies the bodies via
/// the injected `HttpsClient`.
pub fn parse_and_map(
    watch_body: &[u8],
    video_id: &str,
    player_js_body: Option<&[u8]>,
) -> Result<ResolveResponse, ResolverError> {
    let page_state = page::extract(watch_body).map_err(|_| error::malformed())?;
    let selection = payload::select(&page_state.player_response, video_id)?;
    let ops = if selection.needs_cipher() {
        let js_body = player_js_body.ok_or_else(error::malformed)?;
        let js_text = std::str::from_utf8(js_body).map_err(|_| error::malformed())?;
        Some(
            crate::cipher::extract_cipher_ops(js_text)
                .map_err(|_| error::malformed())?,
        )
    } else {
        None
    };
    let finalized = payload::finalize(selection, ops.as_ref())?;
    Ok(mapping::map(finalized))
}

/// Resolve `source` against `client`. Issues one headerless GET for the watch
/// page and, only when at least one candidate format needs signature-cipher
/// decoding, a second headerless GET for the player JS asset.
pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    let id = classify_url(source).map_err(|_| error::invalid_input())?;
    let watch = client
        .get(GetRequest {
            url: id.watch_url(),
            headers: vec![],
        })
        .map_err(error::transport)?;
    validate_https_response(&watch).map_err(|_| error::malformed())?;
    error::status(watch.status)?;

    let page_state = page::extract(&watch.body).map_err(|_| error::malformed())?;
    let selection = payload::select(&page_state.player_response, id.as_str())?;
    let ops = if selection.needs_cipher() {
        let js_url = page_state.player_js_url.clone().ok_or_else(error::malformed)?;
        let js_response = client
            .get(GetRequest {
                url: js_url,
                headers: vec![],
            })
            .map_err(error::transport)?;
        validate_https_response(&js_response).map_err(|_| error::malformed())?;
        error::status(js_response.status)?;
        let js_text = std::str::from_utf8(&js_response.body).map_err(|_| error::malformed())?;
        Some(
            crate::cipher::extract_cipher_ops(js_text)
                .map_err(|_| error::malformed())?,
        )
    } else {
        None
    };
    let finalized = payload::finalize(selection, ops.as_ref())?;
    Ok(mapping::map(finalized))
}

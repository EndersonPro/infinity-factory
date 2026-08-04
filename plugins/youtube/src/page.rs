use bex_media_url_resolver_v2::bounds;
use serde_json::Value;
use std::fmt;

const PLAYER_RESPONSE_MARKER: &str = "var ytInitialPlayerResponse = ";
const PLAYER_RESPONSE_BYTES: usize = 3_145_728;
const JS_URL_MARKER: &str = "\"jsUrl\":\"";
const JS_URL_MAX: usize = 512;

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PageError {
    Invalid,
}
impl fmt::Display for PageError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("YouTube watch page state is unavailable")
    }
}

pub(crate) struct PageState {
    pub(crate) player_response: Value,
    /// Absolute `https://www.youtube.com/...` URL of the player JS asset, if
    /// one was found embedded in the watch page. `None` when the page has no
    /// `jsUrl` field (e.g. an already-cached/blank shell); the caller only
    /// needs this when a format's signature actually requires decoding.
    pub(crate) player_js_url: Option<String>,
}

/// Extract the interior of the first matching bracket/brace pair starting at
/// `s[0]`, skipping over string contents (including escaped quotes) so
/// braces inside JSON string values never desynchronize the scan. Ported
/// from the same balanced-scan technique used by `plugins/bandcamp`'s
/// `data-tralbum` extraction and `bloom-factory/ytvideo`'s cipher parser.
fn extract_braced(s: &str) -> Option<&str> {
    if !s.starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    for (i, c) in s.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        match c {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Takes the first match. Real watch pages routinely embed the marker more
/// than once (e.g. ad/cue-point config blocks re-declaring related globals),
/// so requiring exact uniqueness rejected legitimate, heavily-instrumented
/// pages — mirrors `player_js_url`'s own first-match policy below.
fn selected_player_response(html: &str) -> Result<&str, PageError> {
    let start = html
        .find(PLAYER_RESPONSE_MARKER)
        .ok_or(PageError::Invalid)?;
    let json_start = start + PLAYER_RESPONSE_MARKER.len();
    let json = extract_braced(&html[json_start..]).ok_or(PageError::Invalid)?;
    if json.len() > PLAYER_RESPONSE_BYTES {
        return Err(PageError::Invalid);
    }
    Ok(json)
}

/// Best-effort extraction of the player JS asset URL from a watch page's
/// embedded `ytcfg`/player config (`"jsUrl":"/s/player/.../base.js"`). Takes
/// the first match (mirrors `bloom-factory/ytvideo`'s `fetch_player_js_url`,
/// which does the same); a bounded, quote-delimited scan, not a full parse.
fn player_js_url(html: &str) -> Option<String> {
    let start = html.find(JS_URL_MARKER)? + JS_URL_MARKER.len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    if end > JS_URL_MAX {
        return None;
    }
    let path = &rest[..end];
    if let Some(absolute) = path.strip_prefix("https://www.youtube.com") {
        return Some(format!("https://www.youtube.com{absolute}"));
    }
    if let Some(relative) = path.strip_prefix('/') {
        return Some(format!("https://www.youtube.com/{relative}"));
    }
    None
}

pub(crate) fn extract(body: &[u8]) -> Result<PageState, PageError> {
    if body.len() > bounds::RESPONSE_BODY {
        return Err(PageError::Invalid);
    }
    let html = std::str::from_utf8(body).map_err(|_| PageError::Invalid)?;
    let json = selected_player_response(html)?;
    let player_response: Value = serde_json::from_str(json).map_err(|_| PageError::Invalid)?;
    Ok(PageState {
        player_response,
        player_js_url: player_js_url(html),
    })
}

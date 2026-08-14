use crate::error;
use bex_media_url_resolver_v2::{ResolverError, bounds, resolver_bounds};
use serde_json::{Map, Value};
use url::Url;

/// Parsed view of a Facebook video media object (spec Req 4 / Req 7 / Req 8).
#[derive(Debug)]
pub struct Media {
    pub progressive: Vec<String>,
    pub has_dash_hls: bool,
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub duration_milliseconds: Option<u64>,
}

/// Progressive URL fields collected in source order (spec Req 4).
const PROGRESSIVE_KEYS: &[&str] = &[
    "playable_url",
    "playable_url_quality_hd",
    "browser_native_sd_url",
    "browser_native_hd_url",
];

/// DASH/HLS-only fields that mark a video as `Unsupported` when no progressive
/// URL survives (spec Req 7).
const DASH_HLS_KEYS: &[&str] = &[
    "playable_url_dash",
    "dash_manifest_urls",
    "dash_manifests",
    "hls_playlist_urls",
];

/// Safe HTTPS URL gate reused from the Instagram payload pattern: no userinfo,
/// port, or fragment and ≤2048-byte string (`resolver.rs:43-55`).
fn safe_https(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= bounds::URL
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.fragment().is_none()
        })
}

/// Append `url` preserving source order and deduplicating by value.
fn collect_one(url: &str, out: &mut Vec<String>) {
    if safe_https(url) && out.iter().all(|item| item != url) {
        out.push(url.to_owned());
    }
}

fn collect_progressive_into(obj: &Map<String, Value>, out: &mut Vec<String>) {
    for key in PROGRESSIVE_KEYS {
        if let Some(url) = obj.get(*key).and_then(Value::as_str) {
            collect_one(url, out);
        }
    }
    if let Some(entries) = obj.get("progressive_urls").and_then(Value::as_array) {
        for entry in entries {
            if let Some(url) = entry.get("progressive_url").and_then(Value::as_str) {
                collect_one(url, out);
            }
        }
    }
}

/// Bound a metadata string to `limit` bytes without splitting a UTF-8 codepoint.
fn bounded(value: Option<&str>, limit: usize) -> Option<String> {
    value.map(|text| {
        if text.len() <= limit {
            text.to_owned()
        } else {
            let cut = text
                .char_indices()
                .take_while(|(index, ch)| index + ch.len_utf8() <= limit)
                .last()
                .map(|(index, ch)| index + ch.len_utf8())
                .unwrap_or(0);
            text[..cut].to_owned()
        }
    })
}

fn build_media(media_obj: &Map<String, Value>) -> Media {
    let mut progressive = Vec::new();
    collect_progressive_into(media_obj, &mut progressive);
    progressive.truncate(resolver_bounds::CANDIDATES);
    let has_dash_hls = DASH_HLS_KEYS.iter().any(|key| media_obj.contains_key(*key));
    let title = bounded(
        media_obj.get("title").and_then(Value::as_str),
        resolver_bounds::TITLE,
    );
    let author = media_obj
        .get("owner")
        .and_then(Value::as_object)
        .and_then(|owner| owner.get("name"))
        .and_then(Value::as_str)
        .or_else(|| media_obj.get("uploader").and_then(Value::as_str));
    let author = bounded(author, resolver_bounds::AUTHOR);
    let thumbnail = media_obj
        .get("thumbnail")
        .and_then(Value::as_object)
        .and_then(|thumb| thumb.get("uri"))
        .and_then(Value::as_str)
        .filter(|url| safe_https(url))
        .map(String::from);
    let duration_milliseconds = media_obj
        .get("playable_duration_in_sec")
        .and_then(Value::as_u64)
        .map(|seconds| seconds.saturating_mul(1000));
    Media {
        progressive,
        has_dash_hls,
        title,
        author,
        thumbnail,
        duration_milliseconds,
    }
}

fn follow_playback(creation: Option<&Value>) -> Option<&Map<String, Value>> {
    creation?
        .get("short_form_video_context")?
        .get("playback_video")?
        .as_object()
}

/// Deep-first search for the first JSON object carrying any of `keys`
/// (`yt_dlp/extractor/facebook.py` Relay/GraphQL traversal).
fn find_object_with_keys<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Map<String, Value>> {
    match value {
        Value::Object(obj) => {
            if keys.iter().any(|key| obj.contains_key(*key)) {
                Some(obj)
            } else {
                obj.values()
                    .find_map(|child| find_object_with_keys(child, keys))
            }
        }
        Value::Array(array) => array
            .iter()
            .find_map(|child| find_object_with_keys(child, keys)),
        _ => None,
    }
}

fn search_blocks<'a>(values: &'a [Value], keys: &[&str]) -> Option<&'a Map<String, Value>> {
    for value in values {
        if let Some(found) = find_object_with_keys(value, keys) {
            return Some(found);
        }
    }
    None
}

fn parse_blocks(blocks: &[String]) -> Result<Vec<Value>, ResolverError> {
    blocks
        .iter()
        .map(|block| serde_json::from_str(block).map_err(|_| error::malformed()))
        .collect()
}

/// Parse the `data-sjs` blocks of a Facebook page into an optional `Media`
/// (spec Req 3 / Req 4). An invalid JSON block maps to `malformed-response`;
/// a page whose blocks carry no video media object maps to `Ok(None)` (caller
/// emits `Unsupported`).
pub fn extract_media(blocks: &[String]) -> Result<Option<Media>, ResolverError> {
    let values = parse_blocks(blocks)?;
    // Primary path: a VideoConfig carrying `video_data` with progressive URLs.
    if let Some(container) = search_blocks(&values, &["video_data"])
        && let Some(media_obj) = container.get("video_data").and_then(Value::as_object)
    {
        return Ok(Some(build_media(media_obj)));
    }
    // Reels path: a `creation_story.short_form_video_context.playback_video`
    // carrying progressive URLs when no VideoConfig/video_data is present.
    if let Some(container) = search_blocks(&values, &["creation_story"])
        && let Some(media_obj) = follow_playback(container.get("creation_story"))
    {
        return Ok(Some(build_media(media_obj)));
    }
    Ok(None)
}

/// Parse a Tahoe `post-tahoe` response body, stripping the leading
/// `for (;;);` sentinel before JSON parsing (spec Req 4 / Req 5), then collect
/// `sd_src`/`hd_src` (and the no-ratelimit variants) from the VideoConfig
/// `videoData`. Malformed JSON maps to `malformed-response`.
pub fn parse_tahoe_response(body: &[u8]) -> Result<Vec<String>, ResolverError> {
    let text = std::str::from_utf8(body).map_err(|_| error::malformed())?;
    let stripped = text.strip_prefix("for (;;);").unwrap_or(text);
    let value: Value = serde_json::from_str(stripped).map_err(|_| error::malformed())?;
    let mut urls = Vec::new();
    if let Some(video_data) = find_object_with_keys(
        &value,
        &["sd_src", "hd_src", "sd_src_no_ratelimit", "hd_src_no_ratelimit"],
    ) {
        for key in ["sd_src", "hd_src", "sd_src_no_ratelimit", "hd_src_no_ratelimit"] {
            if let Some(url) = video_data.get(key).and_then(Value::as_str) {
                collect_one(url, &mut urls);
            }
        }
    }
    urls.truncate(resolver_bounds::CANDIDATES);
    Ok(urls)
}
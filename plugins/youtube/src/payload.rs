use crate::cipher::{self, CipherOps};
use crate::error;
use bex_media_url_resolver_v2::ResolverError;
use serde_json::Value;
use url::Url;

const JSON_DEPTH: usize = 64;
const MAX_FORMATS: usize = 64;
const TITLE_LIMIT: usize = 256;
const AUTHOR_LIMIT: usize = 128;
const MIME_LIMIT: usize = 256;
const LABEL_LIMIT: usize = 32;
const URL_LIMIT: usize = 2_048;
const CIPHER_LIMIT: usize = 4_096;

/// Depth-only preflight against the (large, legitimately deep) player
/// response tree: guards against a pathological nesting depth bomb without
/// imposing a member-count cap that would reject real payloads, which
/// routinely carry dozens of formats plus captions/storyboards/ad config.
fn preflight_depth(value: &Value, depth: usize) -> bool {
    if depth > JSON_DEPTH {
        return false;
    }
    match value {
        Value::Array(items) => items.iter().all(|item| preflight_depth(item, depth + 1)),
        Value::Object(map) => map.values().all(|item| preflight_depth(item, depth + 1)),
        _ => true,
    }
}

fn safe_https(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= URL_LIMIT
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.fragment().is_none()
        })
}

fn bounded_string(value: Option<&Value>, limit: usize) -> Result<Option<String>, ResolverError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => {
            if text.len() > limit {
                Err(error::malformed())
            } else {
                Ok(Some(text.clone()))
            }
        }
        Some(_) => Err(error::malformed()),
    }
}

struct RawFormat {
    mime_type: String,
    bitrate: Option<u64>,
    height: Option<u64>,
    quality_label: Option<String>,
    resolved_url: Option<String>,
    cipher: Option<String>,
}

fn parse_format(value: &Value) -> Result<Option<RawFormat>, ResolverError> {
    let obj = value.as_object().ok_or_else(error::malformed)?;
    let mime_type = obj
        .get("mimeType")
        .and_then(Value::as_str)
        .ok_or_else(error::malformed)?;
    if mime_type.is_empty() || mime_type.len() > MIME_LIMIT {
        return Err(error::malformed());
    }
    let bitrate = obj.get("bitrate").and_then(Value::as_u64);
    let height = obj.get("height").and_then(Value::as_u64);
    let quality_label = bounded_string(obj.get("qualityLabel"), LABEL_LIMIT)?;
    let resolved_url = match bounded_string(obj.get("url"), URL_LIMIT)? {
        Some(url) if safe_https(&url) => Some(url),
        Some(_) => return Err(error::malformed()),
        None => None,
    };
    let cipher = bounded_string(
        obj.get("signatureCipher").or_else(|| obj.get("cipher")),
        CIPHER_LIMIT,
    )?;
    if resolved_url.is_none() && cipher.is_none() {
        // DRM'd or otherwise unusable entry (no url and no cipher) — skip it
        // rather than failing the whole request; other formats may still work.
        return Ok(None);
    }
    Ok(Some(RawFormat {
        mime_type: mime_type.to_owned(),
        bitrate,
        height,
        quality_label,
        resolved_url,
        cipher,
    }))
}

fn parse_format_array(value: Option<&Value>) -> Result<Vec<RawFormat>, ResolverError> {
    let Some(value) = value else {
        return Ok(vec![]);
    };
    let items = value.as_array().ok_or_else(error::malformed)?;
    if items.len() > MAX_FORMATS {
        return Err(error::malformed());
    }
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        if let Some(format) = parse_format(item)? {
            out.push(format);
        }
    }
    Ok(out)
}

pub(crate) struct Selection {
    title: Option<String>,
    author: Option<String>,
    thumbnail: Option<String>,
    duration_ms: Option<u64>,
    progressive: Vec<RawFormat>,
    adaptive: Vec<RawFormat>,
}

impl Selection {
    /// Whether resolving this selection requires the player JS (i.e. at
    /// least one candidate format has a `signatureCipher`/`cipher` blob
    /// instead of a direct `url`).
    pub(crate) fn needs_cipher(&self) -> bool {
        self.progressive
            .iter()
            .chain(self.adaptive.iter())
            .any(|format| format.resolved_url.is_none() && format.cipher.is_some())
    }
}

/// Parse and validate a fetched `ytInitialPlayerResponse` value into a
/// bounded intermediate `Selection`. `video_id` (already validated by
/// `classify_url`) is used only to build the standard thumbnail fallback URL
/// when the player response carries no thumbnail list.
pub(crate) fn select(player_response: &Value, video_id: &str) -> Result<Selection, ResolverError> {
    if !preflight_depth(player_response, 0) {
        return Err(error::malformed());
    }
    let status = player_response
        .get("playabilityStatus")
        .and_then(|status| status.get("status"))
        .and_then(Value::as_str)
        .ok_or_else(error::malformed)?;
    match status {
        "OK" => {}
        "LOGIN_REQUIRED" | "CONTENT_CHECK_REQUIRED" | "AGE_CHECK_REQUIRED" => {
            return Err(error::private());
        }
        // Conservative fallback for statuses this table doesn't enumerate
        // (e.g. "ERROR", "UNPLAYABLE", "LIVE_STREAM_OFFLINE", or anything
        // future/unknown): treat as unavailable rather than a parse failure.
        _ => return Err(error::unavailable()),
    }
    let details = player_response.get("videoDetails").and_then(Value::as_object);
    let title = bounded_string(details.and_then(|d| d.get("title")), TITLE_LIMIT)?;
    let author = bounded_string(details.and_then(|d| d.get("author")), AUTHOR_LIMIT)?;
    let duration_ms = details
        .and_then(|d| d.get("lengthSeconds"))
        .and_then(Value::as_str)
        .and_then(|seconds| seconds.parse::<u64>().ok())
        .map(|seconds| seconds.saturating_mul(1000));
    let thumbnail = details
        .and_then(|d| d.get("thumbnail"))
        .and_then(|thumb| thumb.get("thumbnails"))
        .and_then(Value::as_array)
        .and_then(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let url = item.get("url")?.as_str()?;
                    let width = item.get("width").and_then(Value::as_u64).unwrap_or(0);
                    Some((url.to_owned(), width))
                })
                .max_by_key(|(_, width)| *width)
                .map(|(url, _)| url)
        })
        .filter(|url| safe_https(url))
        .or_else(|| Some(format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")));
    let streaming = player_response.get("streamingData").and_then(Value::as_object);
    let progressive = parse_format_array(streaming.and_then(|s| s.get("formats")))?;
    let adaptive = parse_format_array(streaming.and_then(|s| s.get("adaptiveFormats")))?;
    Ok(Selection {
        title,
        author,
        thumbnail,
        duration_ms,
        progressive,
        adaptive,
    })
}

pub(crate) struct ResolvedFormat {
    pub(crate) url: String,
    pub(crate) mime_type: String,
    pub(crate) quality_label: Option<String>,
    pub(crate) bitrate: Option<u64>,
    pub(crate) height: Option<u64>,
}

pub(crate) struct Finalized {
    pub(crate) title: Option<String>,
    pub(crate) author: Option<String>,
    pub(crate) thumbnail: Option<String>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) progressive: Vec<ResolvedFormat>,
    pub(crate) audio: Option<ResolvedFormat>,
    pub(crate) video: Option<ResolvedFormat>,
}

fn resolve_one(raw: RawFormat, ops: Option<&CipherOps>) -> Option<ResolvedFormat> {
    let url = match (raw.resolved_url, raw.cipher) {
        (Some(url), _) => url,
        (None, Some(cipher_str)) => {
            let decoded = cipher::decode_signature_url(&cipher_str, ops?).ok()?;
            if decoded.len() > URL_LIMIT || !safe_https(&decoded) {
                return None;
            }
            decoded
        }
        (None, None) => return None,
    };
    Some(ResolvedFormat {
        url,
        mime_type: raw.mime_type,
        quality_label: raw.quality_label,
        bitrate: raw.bitrate,
        height: raw.height,
    })
}

/// Resolve every pending (cipher-encoded) format using `ops` — required to be
/// `Some` when `selection.needs_cipher()` was true — then rank candidates:
/// progressive (muxed) formats by height/bitrate, and separately the single
/// best adaptive audio and best adaptive video by bitrate/height. Formats
/// whose cipher fails to decode, or whose signature can't be resolved at all
/// (a pending format but no `ops` supplied), are dropped rather than failing
/// the whole request — other formats may still resolve.
pub(crate) fn finalize(
    selection: Selection,
    ops: Option<&CipherOps>,
) -> Result<Finalized, ResolverError> {
    let candidates = bex_media_url_resolver_v2::resolver_bounds::CANDIDATES;
    let mut progressive: Vec<ResolvedFormat> = selection
        .progressive
        .into_iter()
        .filter_map(|format| resolve_one(format, ops))
        .collect();
    progressive.sort_by(|a, b| {
        b.height
            .unwrap_or(0)
            .cmp(&a.height.unwrap_or(0))
            .then(b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)))
    });
    progressive.truncate(candidates);

    let mut audio_raw = Vec::new();
    let mut video_raw = Vec::new();
    for format in selection.adaptive {
        if format.mime_type.starts_with("audio/") {
            audio_raw.push(format);
        } else if format.mime_type.starts_with("video/") {
            video_raw.push(format);
        }
    }
    let mut audio: Vec<ResolvedFormat> = audio_raw
        .into_iter()
        .filter_map(|format| resolve_one(format, ops))
        .collect();
    audio.sort_by(|a, b| b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)));
    let mut video: Vec<ResolvedFormat> = video_raw
        .into_iter()
        .filter_map(|format| resolve_one(format, ops))
        .collect();
    video.sort_by(|a, b| {
        b.height
            .unwrap_or(0)
            .cmp(&a.height.unwrap_or(0))
            .then(b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)))
    });

    Ok(Finalized {
        title: selection.title,
        author: selection.author,
        thumbnail: selection.thumbnail,
        duration_ms: selection.duration_ms,
        progressive,
        audio: audio.into_iter().next(),
        video: video.into_iter().next(),
    })
}

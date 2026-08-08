use crate::error;
use bex_media_url_resolver_v2::ResolverError;
use serde_json::Value;

/// Caps from `compatibility/media-url-resolver-v2-x-policy.json`. A post can
/// carry four attachments and each a handful of renditions; anything beyond
/// these is a payload this resolver does not describe.
const MEDIA_DETAIL_ENTRIES: usize = 4;
const VARIANT_ENTRIES: usize = 16;
const CANDIDATES: usize = 16;
const TITLE_BYTES: usize = 256;
const AUTHOR_BYTES: usize = 128;
const LABEL_BYTES: usize = 64;

const MEDIA_HOST: &str = "video.twimg.com";
const THUMBNAIL_HOST: &str = "pbs.twimg.com";

#[derive(Debug, Eq, PartialEq)]
pub struct Variant {
    /// `{bitrate}` or, when the payload omits it, the rendition dimensions.
    /// Only ever used as a candidate id, never shown.
    pub id: String,
    pub url: String,
    /// `1280x720`, lifted from the rendition path when it is there.
    pub quality_label: Option<String>,
    pub bitrate: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub struct Selection {
    pub variants: Vec<Variant>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail: Option<String>,
    pub duration_ms: Option<u64>,
}

/// Whether `url` is an HTTPS URL on exactly `host`.
///
/// Written against the string rather than a URL parser: this crate has no
/// `url` dependency, the payload is machine-generated, and the check needs to
/// be about the authority alone. A `@` anywhere in the authority means the
/// host is not what it appears to be, so it is refused outright.
fn on_host(url: &str, host: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some(authority) = rest.split(['/', '?', '#']).next() else {
        return false;
    };
    authority == host
}

/// The post's own text, as a title.
///
/// Normalised rather than dropped. Post text routinely runs past the title
/// bound and carries newlines, and `bounded` would answer `None` for both —
/// which would leave most posts with no title at all when the text is the only
/// name they have. Runs of whitespace collapse to one space, and the result is
/// cut on a character boundary.
fn title_of(value: Option<&str>) -> Option<String> {
    let collapsed = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    let mut end = collapsed.len().min(TITLE_BYTES);
    while end > 0 && !collapsed.is_char_boundary(end) {
        end -= 1;
    }
    let trimmed = collapsed[..end].trim_end();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn bounded(value: Option<&str>, limit: usize) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.len() > limit || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.to_owned())
}

/// The `1280x720` segment of a rendition path, when the payload has one.
fn quality_label(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next()?;
    path.split('/')
        .find(|segment| {
            let Some((width, height)) = segment.split_once('x') else {
                return false;
            };
            !width.is_empty()
                && !height.is_empty()
                && width.len() <= 5
                && height.len() <= 5
                && width.bytes().all(|value| value.is_ascii_digit())
                && height.bytes().all(|value| value.is_ascii_digit())
        })
        .filter(|label| label.len() <= LABEL_BYTES)
        .map(str::to_owned)
}

/// Progressive MP4 renditions from one attachment, highest bitrate first.
///
/// HLS variants (`application/x-mpegURL`) are dropped rather than returned.
/// The guest has no manifest parser and the host contract has no notion of
/// one, so handing back a playlist URL as if it were a stream would be a lie
/// the app could not act on.
fn variants_of(media: &Value) -> Vec<Variant> {
    let Some(entries) = media
        .pointer("/video_info/variants")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut variants: Vec<Variant> = entries
        .iter()
        .take(VARIANT_ENTRIES)
        .filter(|variant| variant.get("content_type").and_then(Value::as_str) == Some("video/mp4"))
        .filter_map(|variant| {
            let url = variant.get("url").and_then(Value::as_str)?;
            if url.len() > bex_media_url_resolver_v2::bounds::URL || !on_host(url, MEDIA_HOST) {
                return None;
            }
            let bitrate = variant.get("bitrate").and_then(Value::as_u64).unwrap_or(0);
            let quality_label = quality_label(url);
            Some(Variant {
                id: quality_label
                    .clone()
                    .unwrap_or_else(|| format!("{bitrate}")),
                url: url.to_owned(),
                quality_label,
                bitrate,
            })
        })
        .collect();

    // Highest bitrate first, so a caller taking the head takes the best.
    variants.sort_by(|left, right| right.bitrate.cmp(&left.bitrate));
    variants
}

/// Parse a syndication body into the streams and metadata worth returning.
///
/// `Ok(None)` means the post is real but carries no native video — a text
/// post, a photo, a link card. That is the common answer, not a failure:
/// roughly two in five public posts sampled had nothing to resolve.
pub fn parse(body: &[u8]) -> Result<Option<Selection>, ResolverError> {
    let root: Value = serde_json::from_slice(body).map_err(|_| error::malformed())?;
    if !root.is_object() {
        return Err(error::malformed());
    }

    // A withdrawn post, or one whose account has since been protected. The
    // endpoint answers 200 with a tombstone rather than a status code.
    if root.get("__typename").and_then(Value::as_str) == Some("TweetTombstone") {
        return Err(error::private());
    }

    let Some(details) = root.get("mediaDetails").and_then(Value::as_array) else {
        return Ok(None);
    };

    // The first attachment that carries video, and only that one.
    //
    // A post can hold four separate videos. `candidates` means renditions of
    // one medium so the caller can choose a quality; flattening four videos
    // into that list would have the app pick "the best" and download an
    // arbitrary one of them. One capture, one video, and it is the first —
    // which is the one the link previews and the one the person pasting it
    // was looking at.
    let Some((media, variants)) = details
        .iter()
        .take(MEDIA_DETAIL_ENTRIES)
        .map(|media| (media, variants_of(media)))
        .find(|(_, variants)| !variants.is_empty())
    else {
        return Ok(None);
    };

    // Distinct URLs, capped to the response limit. Labels are unique within
    // one attachment, so ids need no disambiguation once the selection is
    // scoped to a single medium.
    let mut distinct: Vec<Variant> = Vec::new();
    for variant in variants {
        if distinct.len() == CANDIDATES
            || distinct
                .iter()
                .any(|kept| kept.url == variant.url || kept.id == variant.id)
        {
            continue;
        }
        distinct.push(variant);
    }

    Ok(Some(Selection {
        variants: distinct,
        title: title_of(root.get("text").and_then(Value::as_str)),
        author: bounded(
            root.pointer("/user/name").and_then(Value::as_str),
            AUTHOR_BYTES,
        ),
        thumbnail: media
            .get("media_url_https")
            .and_then(Value::as_str)
            .filter(|url| on_host(url, THUMBNAIL_HOST))
            .map(str::to_owned),
        duration_ms: media
            .pointer("/video_info/duration_millis")
            .and_then(Value::as_u64),
    }))
}

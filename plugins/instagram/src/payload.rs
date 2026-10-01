use crate::error;
use bex_media_url_resolver_v2::{ResolverError, resolver_bounds};
use serde_json::Value;
use std::collections::HashSet;
use url::Url;

pub(crate) enum Node {
    Video { url: String },
    Image { url: String },
}
impl Node {
    pub(crate) fn url(&self) -> &str {
        match self {
            Self::Video { url } | Self::Image { url } => url,
        }
    }
}
pub(crate) struct Payload {
    pub root: Node,
    pub children: Option<Vec<Node>>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub thumbnail: String,
}
fn safe_https(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 2_048
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.fragment().is_none()
        })
}
fn selected_string(
    value: &Value,
    object: &str,
    field: &str,
) -> Result<Option<String>, ResolverError> {
    let Some(parent) = value.get(object).filter(|item| !item.is_null()) else {
        return Ok(None);
    };
    let parent = parent.as_object().ok_or_else(error::malformed)?;
    match parent.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|item| Some(item.to_owned()))
            .ok_or_else(error::malformed),
    }
}
fn https_at<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|url| safe_https(url))
}
/// Best still image: the first `image_versions2` candidate of the current
/// shape, then its `display_uri`, then the legacy `display_url`.
fn image(value: &Value) -> Result<&str, ResolverError> {
    https_at(value, "/image_versions2/candidates/0/url")
        .or_else(|| https_at(value, "/display_uri"))
        .or_else(|| https_at(value, "/display_url"))
        .ok_or_else(error::malformed)
}
fn node(value: &Value) -> Result<Node, ResolverError> {
    value.as_object().ok_or_else(error::malformed)?;
    let still = image(value)?;
    // Current shape: `media_type` 2 carries `video_versions` (best first);
    // legacy shape: `is_video` with `video_url`.
    let is_video = match value.get("media_type") {
        Some(kind) => kind.as_u64().ok_or_else(error::malformed)? == 2,
        None => value
            .get("is_video")
            .and_then(Value::as_bool)
            .ok_or_else(error::malformed)?,
    };
    if !is_video {
        return Ok(Node::Image { url: still.into() });
    }
    let video = https_at(value, "/video_versions/0/url")
        .or_else(|| https_at(value, "/video_url"))
        .ok_or_else(error::malformed)?;
    Ok(Node::Video { url: video.into() })
}
fn children(value: &Value) -> Result<Option<Vec<Node>>, ResolverError> {
    // Current shape: `carousel_media`; legacy: `edge_sidecar_to_children`.
    let present = |key| value.get(key).filter(|item: &&Value| !item.is_null());
    let items: Vec<&Value> = if let Some(media) = present("carousel_media") {
        media
            .as_array()
            .ok_or_else(error::malformed)?
            .iter()
            .collect()
    } else if let Some(sidecar) = present("edge_sidecar_to_children") {
        sidecar
            .get("edges")
            .and_then(Value::as_array)
            .ok_or_else(error::malformed)?
            .iter()
            .map(|edge| edge.get("node").ok_or_else(error::malformed))
            .collect::<Result<_, _>>()?
    } else {
        return Ok(None);
    };
    if items.is_empty() || items.len() > resolver_bounds::CANDIDATES {
        return Err(error::malformed());
    }
    let mut seen = HashSet::new();
    let mut nodes = Vec::with_capacity(items.len());
    for item in items {
        let child = node(item)?;
        if !seen.insert(child.url().to_owned()) {
            return Err(error::malformed());
        }
        nodes.push(child);
    }
    Ok(Some(nodes))
}
/// Captions are free, multi-line text that routinely exceeds the SDK title
/// bound, and the SDK rejects control characters. Collapse whitespace and
/// control runs to one space, then shorten on a char boundary instead of
/// rejecting the post.
fn title(caption: &str) -> Option<String> {
    let words = caption.split(|item: char| item.is_whitespace() || item.is_control());
    let joined = words
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let bounded = &joined[..joined.floor_char_boundary(resolver_bounds::TITLE)];
    (!bounded.is_empty()).then(|| bounded.trim_end().to_owned())
}
fn selected_payload(value: &Value) -> Result<Payload, ResolverError> {
    let root = node(value)?;
    let thumbnail = image(value)?.to_owned();
    let title = selected_string(value, "caption", "text")?.and_then(|text| title(&text));
    let author = match selected_string(value, "user", "username")? {
        Some(author) => Some(author),
        None => selected_string(value, "owner", "username")?,
    };
    if author
        .as_ref()
        .is_some_and(|item| item.len() > resolver_bounds::AUTHOR)
    {
        return Err(error::malformed());
    }
    Ok(Payload {
        root,
        children: children(value)?,
        title,
        author,
        thumbnail,
    })
}
pub(crate) fn graphql(body: &[u8]) -> Result<Payload, ResolverError> {
    let value: Value = serde_json::from_slice(body).map_err(|_| error::malformed())?;
    let media = value
        .get("data")
        .and_then(|item| item.get("xig_polaris_media"))
        .ok_or_else(error::malformed)?;
    if media.get("is_private").and_then(Value::as_bool) == Some(true) {
        return Err(error::private());
    }
    if media.get("is_unavailable").and_then(Value::as_bool) == Some(true) {
        return Err(error::unavailable());
    }
    // A logged-out viewer gets `if_not_gated_logged_out: null` plus a
    // `gating_ruling` (e.g. age restriction) when the post needs an account.
    let gated = media
        .get("gating_ruling")
        .is_some_and(|item| !item.is_null());
    match media
        .get("if_not_gated_logged_out")
        .filter(|item| !item.is_null())
    {
        Some(item) => selected_payload(item).map_err(|failure| {
            if gated {
                error::signed_in_required()
            } else {
                failure
            }
        }),
        None if gated => Err(error::signed_in_required()),
        None => Err(error::malformed()),
    }
}
pub(crate) fn relay(body: &str) -> Result<Payload, ResolverError> {
    let value: Value = serde_json::from_str(body).map_err(|_| error::malformed())?;
    selected_payload(value.get("media").ok_or_else(error::malformed)?)
}

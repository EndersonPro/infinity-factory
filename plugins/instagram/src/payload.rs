use crate::error;
use bex_media_url_resolver_v2::ResolverError;
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
    let Some(parent) = value.get(object) else {
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
fn node(value: &Value) -> Result<Node, ResolverError> {
    let value = value.as_object().ok_or_else(error::malformed)?;
    let is_video = value
        .get("is_video")
        .and_then(Value::as_bool)
        .ok_or_else(error::malformed)?;
    let display = value
        .get("display_url")
        .and_then(Value::as_str)
        .filter(|url| safe_https(url))
        .ok_or_else(error::malformed)?;
    if is_video {
        let video = value
            .get("video_url")
            .and_then(Value::as_str)
            .filter(|url| safe_https(url))
            .ok_or_else(error::malformed)?;
        Ok(Node::Video { url: video.into() })
    } else {
        Ok(Node::Image {
            url: display.into(),
        })
    }
}
fn selected_payload(value: &Value) -> Result<Payload, ResolverError> {
    let root = node(value)?;
    let thumbnail = value
        .get("display_url")
        .and_then(Value::as_str)
        .filter(|url| safe_https(url))
        .ok_or_else(error::malformed)?
        .to_owned();
    let title = selected_string(value, "caption", "text")?;
    let author = selected_string(value, "owner", "username")?;
    if title.as_ref().is_some_and(|item| item.len() > 256)
        || author.as_ref().is_some_and(|item| item.len() > 128)
    {
        return Err(error::malformed());
    }
    let children = match value.get("edge_sidecar_to_children") {
        None => None,
        Some(sidecar) => {
            let edges = sidecar
                .get("edges")
                .and_then(Value::as_array)
                .filter(|items| !items.is_empty() && items.len() <= 16)
                .ok_or_else(error::malformed)?;
            let mut seen = HashSet::new();
            let mut children = Vec::with_capacity(edges.len());
            for edge in edges {
                let child = node(edge.get("node").ok_or_else(error::malformed)?)?;
                if !seen.insert(child.url().to_owned()) {
                    return Err(error::malformed());
                }
                children.push(child);
            }
            Some(children)
        }
    };
    Ok(Payload {
        root,
        children,
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
    selected_payload(
        media
            .get("if_not_gated_logged_out")
            .ok_or_else(error::malformed)?,
    )
}
pub(crate) fn relay(body: &str) -> Result<Payload, ResolverError> {
    let value: Value = serde_json::from_str(body).map_err(|_| error::malformed())?;
    selected_payload(value.get("media").ok_or_else(error::malformed)?)
}

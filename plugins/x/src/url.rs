use bex_media_url_resolver_v2::bounds;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public X post URL")
    }
}

/// The numeric post id lifted out of an accepted URL.
///
/// Held as the digits rather than a `u64` so the string that goes into the
/// request query is the one that was classified, with no reformatting between
/// the two.
#[derive(Debug, Eq, PartialEq)]
pub struct PostId(String);
impl PostId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The numeric value, for the token derivation. Infallible for an accepted
    /// id: classification bounds it to 19 digits.
    pub fn as_u64(&self) -> u64 {
        self.0.parse().unwrap_or_default()
    }
}

/// Hosts that serve the same public post. `x.com` and `twitter.com` are the
/// live and legacy spellings of one site, and `mobile.` is what a phone share
/// sheet still produces.
fn known_host(host: &str) -> bool {
    matches!(
        host,
        "x.com" | "www.x.com" | "twitter.com" | "www.twitter.com" | "mobile.twitter.com"
    )
}

/// 1-15 characters of `A-Za-z0-9_`, which is the handle X itself allows.
fn valid_handle(handle: &str) -> bool {
    !handle.is_empty()
        && handle.len() <= 15
        && handle
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_')
}

/// 1-19 digits, no leading zero. Bounded so the value always fits a `u64`,
/// which the token derivation needs.
fn valid_post_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 19
        && !id.starts_with('0')
        && id.bytes().all(|value| value.is_ascii_digit())
}

/// Accept `https://{host}/{handle}/status/{id}` and lift out the id.
///
/// A trailing `/photo/{n}` or `/video/{n}` is accepted because that is what a
/// tap on a specific attachment produces, and it names a view of the same post
/// rather than a different one. A query string is ignored rather than
/// rejected: every share sheet appends `?t=…&s=…`, and refusing those would
/// make the resolver useless on the links people actually paste. Neither the
/// query nor the trailing segment reaches the network — the request URL is
/// built from the id alone.
pub fn classify_url(source: &str) -> Result<PostId, UrlError> {
    if source.is_empty() || source.len() > bounds::URL || !source.is_ascii() {
        return Err(UrlError::Invalid);
    }
    let rest = source.strip_prefix("https://").ok_or(UrlError::Invalid)?;
    // Cut the query and fragment before parsing the path; neither participates
    // in identifying the post.
    let rest = rest.split(['?', '#']).next().ok_or(UrlError::Invalid)?;
    let (host, path) = rest.split_once('/').ok_or(UrlError::Invalid)?;

    if host.contains(['@', ':']) || !known_host(host) {
        return Err(UrlError::Invalid);
    }

    let mut parts = path.split('/');
    let handle = parts.next().ok_or(UrlError::Invalid)?;
    let status = parts.next().ok_or(UrlError::Invalid)?;
    let id = parts.next().ok_or(UrlError::Invalid)?;

    if !valid_handle(handle) || status != "status" || !valid_post_id(id) {
        return Err(UrlError::Invalid);
    }

    // Whatever follows the id may only be an attachment view, optionally with
    // a trailing slash. Anything else is a different page.
    match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (None, ..) => {}
        (Some(""), None, ..) => {}
        (Some("photo" | "video"), Some(index), tail, None)
            if !index.is_empty()
                && index.len() <= 2
                && index.bytes().all(|value| value.is_ascii_digit())
                && matches!(tail, None | Some("")) => {}
        _ => return Err(UrlError::Invalid),
    }

    Ok(PostId(id.to_owned()))
}

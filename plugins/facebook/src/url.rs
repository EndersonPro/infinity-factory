use bex_media_url_resolver_v2::bounds;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public Facebook URL")
    }
}

/// A canonical, classiffied public Facebook video URL plus the extracted
/// video identifier used for the Tahoe fallback target
/// (`https://www.facebook.com/video/tahoe/async/{id}/`).
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CanonicalUrl {
    url: String,
    video_id: String,
}
impl CanonicalUrl {
    pub fn as_str(&self) -> &str {
        &self.url
    }
    pub fn video_id(&self) -> &str {
        &self.video_id
    }
}

/// Facebook user label: 1..=64 bytes from `[A-Za-z0-9._-]`
/// (`yt_dlp/extractor/facebook.py:34-58`; spec Req 1).
fn valid_user_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Facebook video id: either all ASCII digits, or a `pfbid{base64}` body drawn
/// from `[A-Za-z0-9_-]` (spec Req 1 scenario "Accepts pfbid base64 identifier").
fn valid_video_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && (id.bytes().all(|byte| byte.is_ascii_digit()) || {
            id.len() > 5
                && id.starts_with("pfbid")
                && id[5..]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        })
}

fn authority_allowed(authority: &str) -> bool {
    matches!(authority, "www.facebook.com" | "facebook.com")
}

/// Classify a source URL as a canonical public Facebook video page (spec Req 1).
///
/// Admits `/{user}/videos/{id}/`, `/watch/?v={id}`, `/{user}/reels/{id}/`,
/// `/{user}/reel/{id}/`, and `/reel/{id}/`. Rejects posts, groups, stories,
/// `facebook:{id}`, `/video.php`, `/plugins/video.php`, embeds, non-`www` hosts
/// (`m.`, `mbasic.`, `web.`), userinfo, ports, and fragments before any host
/// call. The canonical form always normalises the authority to `www.facebook`
/// and carries the extracted video id for the Tahoe target.
pub fn classify_url(source: &str) -> Result<CanonicalUrl, UrlError> {
    if source.is_empty()
        || source.len() > bounds::URL
        || !source.is_ascii()
        || source.contains('#')
    {
        return Err(UrlError::Invalid);
    }
    if !source.starts_with("https://") {
        return Err(UrlError::Invalid);
    }
    let rest = &source["https://".len()..];
    let (authority, path_and_query) = rest.split_once('/').ok_or(UrlError::Invalid)?;
    if !authority_allowed(authority) || authority.contains('@') || authority.contains(':') {
        return Err(UrlError::Invalid);
    }
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    let segments: Vec<&str> = path.split('/').filter(|item| !item.is_empty()).collect();

    // `/watch/?v={id}` — exactly one `v` query parameter carrying the id.
    if segments.as_slice() == ["watch"] {
        let Some(query) = query else {
            return Err(UrlError::Invalid);
        };
        let Some(id) = query.strip_prefix("v=") else {
            return Err(UrlError::Invalid);
        };
        if query.contains('&') || !valid_video_id(id) {
            return Err(UrlError::Invalid);
        }
        return Ok(CanonicalUrl {
            url: format!("https://www.facebook.com/watch/?v={id}"),
            video_id: id.into(),
        });
    }
    // Path-style canonical URLs admit NO query string.
    if query.is_some() {
        return Err(UrlError::Invalid);
    }
    let (user, family, id): (&str, &str, &str) = match segments.as_slice() {
        [user, "videos", id] if valid_user_label(user) => (user, "videos", id),
        [user, "reels", id] if valid_user_label(user) => (user, "reels", id),
        [user, "reel", id] if valid_user_label(user) => (user, "reel", id),
        ["reel", id] => ("", "reel", id),
        _ => return Err(UrlError::Invalid),
    };
    if !valid_video_id(id) {
        return Err(UrlError::Invalid);
    }
    let canonical = match family {
        "videos" => format!("https://www.facebook.com/{user}/videos/{id}/"),
        "reels" => format!("https://www.facebook.com/{user}/reels/{id}/"),
        "reel" if user.is_empty() => format!("https://www.facebook.com/reel/{id}/"),
        "reel" => format!("https://www.facebook.com/{user}/reel/{id}/"),
        _ => return Err(UrlError::Invalid),
    };
    Ok(CanonicalUrl {
        url: canonical,
        video_id: id.into(),
    })
}
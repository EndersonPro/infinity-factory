use bex_media_url_resolver_v2::bounds;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public TikTok URL")
    }
}
#[derive(Debug, Eq, PartialEq)]
pub struct CanonicalUrl(String);
impl CanonicalUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// TikTok `@{user}` label: 1-64 bytes from `[A-Za-z0-9._-]`. The literal `_`
/// is a canonical user segment (yt-dlp's `_create_url` fallback at
/// `yt_dlp/extractor/tiktok.py:106-108`), not a sentinel; it is admitted by
/// this character set and MUST NOT be special-cased.
fn valid_user_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
}

/// TikTok video id: 1-19 ASCII digits.
fn valid_video_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 19 && id.bytes().all(|value| value.is_ascii_digit())
}

/// Spec Req 1: Accepts canonical user/video URL; Accepts canonical user "_"
/// sentinel URL (the `_` is a normal user, not a sentinel); Accepts usernames
/// with dots and underscores; Rejects query/fragment/userinfo/port and every
/// non-canonical host/shortlink/profile/live/douyin source before any host
/// call (`openspec/changes/add-tiktok-resolver/specs/tiktok-video-resolution/
/// spec.md:15-93`). Canonical shape: `https://www.tiktok.com/@{user}/video/{id}`.
pub fn classify_url(source: &str) -> Result<CanonicalUrl, UrlError> {
    if source.is_empty()
        || source.len() > bounds::URL
        || !source.is_ascii()
        || source.contains('#')
        || source.contains('?')
    {
        return Err(UrlError::Invalid);
    }
    let rest = source.strip_prefix("https://").ok_or(UrlError::Invalid)?;
    let (authority, path) = rest.split_once('/').ok_or(UrlError::Invalid)?;
    if authority.contains('@') || authority.contains(':') || authority != "www.tiktok.com" {
        return Err(UrlError::Invalid);
    }
    let segments: Vec<&str> = path.split('/').collect();
    if segments.len() != 3 {
        return Err(UrlError::Invalid);
    }
    let user = segments[0];
    if !user.starts_with('@') || user.len() < 2 || !valid_user_label(&user[1..]) {
        return Err(UrlError::Invalid);
    }
    if segments[1] != "video" {
        return Err(UrlError::Invalid);
    }
    if !valid_video_id(segments[2]) {
        return Err(UrlError::Invalid);
    }
    Ok(CanonicalUrl(source.to_owned()))
}

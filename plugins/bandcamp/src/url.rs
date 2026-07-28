use bex_media_url_resolver_v2::bounds;
use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public Bandcamp URL")
    }
}
#[derive(Debug, Eq, PartialEq)]
pub struct CanonicalUrl(String);
impl CanonicalUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// RFC 1035 lowercase DNS label: 1-63 bytes, `[a-z0-9-]`, no leading/trailing
/// hyphen, and no consecutive hyphens at the boundaries used by public Bandcamp
/// artist subdomains (`{artist}.bandcamp.com`).
fn valid_artist_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 63
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !label.starts_with('-')
        && !label.ends_with('-')
}

/// Lowercase alphanumeric/hyphen slug, 1-128 bytes.
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

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
    if rest.contains('@') || rest.contains(':') {
        return Err(UrlError::Invalid);
    }
    let (authority, path) = rest.split_once('/').ok_or(UrlError::Invalid)?;
    let artist = authority
        .strip_suffix(".bandcamp.com")
        .filter(|label| *label != "www" && valid_artist_label(label))
        .ok_or(UrlError::Invalid)?;
    let _ = artist;
    let segments: Vec<&str> = path.split('/').collect();
    if !matches!(segments.as_slice(), ["track", slug] if valid_slug(slug)) {
        return Err(UrlError::Invalid);
    }
    Ok(CanonicalUrl(source.to_owned()))
}

use bex_media_url_resolver_v2::bounds;
use std::{collections::HashSet, fmt};

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public Instagram URL")
    }
}
#[derive(Debug, Eq, PartialEq)]
pub struct CanonicalUrl(String);
impl CanonicalUrl {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
fn valid_query(query: &str) -> bool {
    if query.is_empty() || query.contains('?') {
        return false;
    }
    let mut seen = HashSet::new();
    query.split('&').all(|pair| {
        let Some((key, value)) = pair.split_once('=') else {
            return false;
        };
        matches!(key, "igsh" | "utm_source" | "utm_medium")
            && seen.insert(key)
            && !value.is_empty()
            && value.len() <= 256
            && value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'=')
            })
    })
}
pub fn classify_url(source: &str) -> Result<CanonicalUrl, UrlError> {
    if source.is_empty() || source.len() > bounds::URL || !source.is_ascii() || source.contains('#')
    {
        return Err(UrlError::Invalid);
    }
    let rest = source.strip_prefix("https://").ok_or(UrlError::Invalid)?;
    let (authority, path_and_query) = rest.split_once('/').ok_or(UrlError::Invalid)?;
    if !matches!(authority, "instagram.com" | "www.instagram.com") {
        return Err(UrlError::Invalid);
    }
    let (path, query) = path_and_query
        .split_once('?')
        .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
    if query.is_some_and(|value| !valid_query(value)) {
        return Err(UrlError::Invalid);
    }
    let parts: Vec<_> = path.split('/').collect();
    if !matches!(parts.as_slice(), [_, _] | [_, _, ""]) {
        return Err(UrlError::Invalid);
    }
    let family = parts[0];
    let identifier = parts[1];
    if !matches!(family, "p" | "reel" | "reels" | "tv")
        || identifier.is_empty()
        || identifier.len() > 64
        || !identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(UrlError::Invalid);
    }
    Ok(CanonicalUrl(format!(
        "https://www.instagram.com/{family}/{identifier}/"
    )))
}

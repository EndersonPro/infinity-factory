use bex_media_url_resolver_v2::bounds;
use std::fmt;

const ID_LEN: usize = 11;

#[derive(Debug, Eq, PartialEq)]
pub enum UrlError {
    Invalid,
}
impl fmt::Display for UrlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("unsupported public YouTube URL")
    }
}

/// A validated 11-character YouTube video id, always resolved against the
/// canonical `www.youtube.com/watch` shape regardless of which supported
/// input host/path it was classified from (mirrors Instagram's
/// canonicalize-to-`www.instagram.com` precedent).
#[derive(Debug, Eq, PartialEq)]
pub struct VideoId(String);
impl VideoId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn watch_url(&self) -> String {
        format!("https://www.youtube.com/watch?v={}", self.0)
    }
}

fn valid_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Classifies a public YouTube URL strictly, before any I/O. Accepted shapes:
/// `https://{youtube.com,www.youtube.com,m.youtube.com}/watch?v={id}`,
/// `https://{youtube.com,www.youtube.com,m.youtube.com}/shorts/{id}`, and
/// `https://youtu.be/{id}`. Fragments and unsupported path shapes are
/// rejected with zero host calls; any other query parameter (`si`, `list`,
/// `t`, `feature`, `pp`, ...) is share-link noise, not part of the shape, and
/// is ignored rather than rejected — a real share from the YouTube app
/// almost always carries an `si` param on `youtu.be` links.
pub fn classify_url(source: &str) -> Result<VideoId, UrlError> {
    if source.is_empty() || source.len() > bounds::URL || !source.is_ascii() || source.contains('#')
    {
        return Err(UrlError::Invalid);
    }
    let rest = source.strip_prefix("https://").ok_or(UrlError::Invalid)?;
    if rest.contains('@') || rest.contains(':') {
        return Err(UrlError::Invalid);
    }
    let (authority, path_and_query) = rest.split_once('/').ok_or(UrlError::Invalid)?;
    let candidate = if matches!(
        authority,
        "www.youtube.com" | "youtube.com" | "m.youtube.com"
    ) {
        let (path, query) = path_and_query
            .split_once('?')
            .map_or((path_and_query, None), |(path, query)| (path, Some(query)));
        let segments: Vec<&str> = path.split('/').collect();
        if matches!(segments.as_slice(), ["watch"]) {
            // `v` need not be the only or the first query param.
            query
                .ok_or(UrlError::Invalid)?
                .split('&')
                .find_map(|pair| pair.strip_prefix("v="))
                .ok_or(UrlError::Invalid)?
        } else if matches!(segments.as_slice(), ["shorts", _] | ["shorts", _, ""]) {
            segments[1]
        } else {
            return Err(UrlError::Invalid);
        }
    } else if authority == "youtu.be" {
        let path = path_and_query
            .split_once('?')
            .map_or(path_and_query, |(path, _)| path);
        let segments: Vec<&str> = path.split('/').collect();
        if matches!(segments.as_slice(), [_]) {
            segments[0]
        } else {
            return Err(UrlError::Invalid);
        }
    } else {
        return Err(UrlError::Invalid);
    };
    if !valid_id(candidate) {
        return Err(UrlError::Invalid);
    }
    Ok(VideoId(candidate.to_owned()))
}

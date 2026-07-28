use crate::error;
use bex_media_url_resolver_v2::ResolverError;
use serde_json::Value;
use url::Url;

const JSON_DEPTH: usize = 32;
const JSON_MEMBERS: usize = 512;
const JSON_STRING: usize = 4_096;
const TRACKINFO_LIMIT: usize = 16;
const FILE_LIMIT: usize = 16;
const FORMAT_ID_LIMIT: usize = 32;
const MEDIA_URL_LIMIT: usize = 2_048;
const TITLE_LIMIT: usize = 256;
const ARTIST_LIMIT: usize = 128;

pub struct RetainedFormat {
    pub format_id: String,
    pub url: String,
    pub expires_at: Option<u64>,
}

pub struct Selection {
    pub formats: Vec<RetainedFormat>,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub duration_ms: Option<u64>,
    pub thumbnail: Option<String>,
}

fn safe_https(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MEDIA_URL_LIMIT
        && Url::parse(value).is_ok_and(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.port().is_none()
                && url.fragment().is_none()
        })
}

struct Limiter {
    members: usize,
}

fn preflight(value: &Value, depth: usize, lim: &mut Limiter) -> bool {
    match value {
        Value::String(text) => text.len() <= JSON_STRING,
        Value::Array(items) => {
            if depth > JSON_DEPTH {
                return false;
            }
            lim.members += items.len();
            if lim.members > JSON_MEMBERS {
                return false;
            }
            items.iter().all(|item| preflight(item, depth + 1, lim))
        }
        Value::Object(map) => {
            if depth > JSON_DEPTH {
                return false;
            }
            lim.members += map.len();
            if lim.members > JSON_MEMBERS {
                return false;
            }
            map.iter().all(|(_, item)| preflight(item, depth + 1, lim))
        }
        _ => true,
    }
}

fn as_i64(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| value.as_u64().map(|u| u as i64))
}

/// Parse the `ts`/`token` corroboration in a media URL's query string into the
/// stream expiry. `ts` absent means no expiry; a `ts` marker without a
/// corroborating `token` prefix, or a malformed/non-unique `ts`, is a
/// conflicting marker that fails closed as `MalformedResponse`.
fn parse_expiry(url: &str) -> Result<Option<u64>, ResolverError> {
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or("");
    if query.is_empty() {
        return Ok(None);
    }
    let mut ts_values: Vec<&str> = Vec::new();
    let mut token_values: Vec<&str> = Vec::new();
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            return Err(error::malformed());
        };
        match key {
            "ts" => ts_values.push(value),
            "token" => token_values.push(value),
            _ => {}
        }
    }
    if ts_values.is_empty() {
        return Ok(None);
    }
    if ts_values.len() > 1 || token_values.len() > 1 {
        return Err(error::malformed());
    }
    let ts = ts_values[0];
    if !ts.bytes().all(|b| b.is_ascii_digit()) || ts.is_empty() {
        return Err(error::malformed());
    }
    let token = match token_values.first() {
        Some(token) => *token,
        None => return Err(error::malformed()),
    };
    if !token.starts_with(ts) {
        return Err(error::malformed());
    }
    let parsed = ts.parse::<u64>().map_err(|_| error::malformed())?;
    Ok(Some(parsed))
}

fn selected_string(value: &Value, max: usize) -> Result<Option<String>, ResolverError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) => {
            if text.len() > max {
                return Err(error::malformed());
            }
            Ok(Some(text.clone()))
        }
        _ => Err(error::malformed()),
    }
}

pub fn parse(json: &str, now: u64, thumbnail: Option<&str>) -> Result<Selection, ResolverError> {
    let value: Value = serde_json::from_str(json).map_err(|_| error::malformed())?;
    let mut lim = Limiter { members: 0 };
    if !preflight(&value, 0, &mut lim) {
        return Err(error::malformed());
    }
    let current = value
        .get("current")
        .and_then(Value::as_object)
        .ok_or_else(error::malformed)?;
    let current_id = current
        .get("id")
        .and_then(as_i64)
        .ok_or_else(error::malformed)?;
    let trackinfo = value
        .get("trackinfo")
        .and_then(Value::as_array)
        .ok_or_else(error::malformed)?;
    if trackinfo.len() > TRACKINFO_LIMIT {
        return Err(error::malformed());
    }
    if trackinfo.is_empty() {
        return Err(error::unavailable());
    }
    let mut matching: Vec<&Value> = Vec::new();
    for entry in trackinfo {
        let entry_obj = entry.as_object().ok_or_else(error::malformed)?;
        let entry_id = entry_obj
            .get("track_id")
            .and_then(as_i64)
            .or_else(|| entry_obj.get("id").and_then(as_i64));
        if entry_id == Some(current_id) {
            matching.push(entry);
        }
    }
    if matching.len() != 1 {
        return Err(error::malformed());
    }
    let selected = matching[0].as_object().ok_or_else(error::malformed)?;
    let file = selected
        .get("file")
        .and_then(Value::as_object)
        .ok_or_else(error::malformed)?;
    if file.len() > FILE_LIMIT {
        return Err(error::malformed());
    }
    if file.is_empty() {
        return Err(error::unavailable());
    }
    let mut formats: Vec<RetainedFormat> = Vec::new();
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (format_id, media) in file.iter() {
        if format_id.len() > FORMAT_ID_LIMIT {
            return Err(error::malformed());
        }
        let url = match media.as_str() {
            Some("") => continue,
            Some(url) => url,
            None => return Err(error::malformed()),
        };
        if !safe_https(url) {
            return Err(error::malformed());
        }
        if !seen_urls.insert(url.to_owned()) {
            continue;
        }
        let expires_at = parse_expiry(url)?;
        if expires_at.is_some_and(|expiry| expiry <= now) {
            continue;
        }
        formats.push(RetainedFormat {
            format_id: format_id.clone(),
            url: url.to_owned(),
            expires_at,
        });
    }
    if formats.is_empty() {
        return Err(error::unavailable());
    }
    if formats.len() > FILE_LIMIT {
        return Err(error::malformed());
    }
    let title = selected_string(selected.get("title").unwrap_or(&Value::Null), TITLE_LIMIT)?;
    let artist = selected_string(current.get("artist").unwrap_or(&Value::Null), ARTIST_LIMIT)?
        .or_else(|| {
            selected
                .get("artist")
                .and_then(Value::as_str)
                .filter(|a| a.len() <= ARTIST_LIMIT)
                .map(str::to_owned)
        })
        .or_else(|| {
            value
                .get("artist")
                .and_then(Value::as_str)
                .filter(|a| a.len() <= ARTIST_LIMIT)
                .map(str::to_owned)
        });
    let duration_ms = selected
        .get("duration")
        .and_then(Value::as_f64)
        .map(|seconds| (seconds * 1000.0).round().max(0.0) as u64);
    let thumbnail = thumbnail
        .filter(|t| safe_https(t) && t.len() <= MEDIA_URL_LIMIT)
        .map(str::to_owned);
    Ok(Selection {
        formats,
        title,
        artist,
        duration_ms,
        thumbnail,
    })
}

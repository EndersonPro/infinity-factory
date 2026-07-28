use bex_media_url_resolver_v2::bounds;
use std::fmt;

const MARKER: &str = "data-tralbum=";
const ENCODED_LIMIT: usize = 1_048_576;
const DECODED_LIMIT: usize = 1_048_576;

#[derive(Debug, Eq, PartialEq)]
pub enum HtmlError {
    Invalid,
}
impl fmt::Display for HtmlError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("Bandcamp page state is unavailable")
    }
}

pub struct TralbumData {
    pub json: String,
    pub thumbnail: Option<String>,
}

/// Locate the unique `data-tralbum="..."` attribute, returning the byte range
/// of its encoded value and the quote character that delimits it.
fn locate_tralbum(html: &str) -> Result<(usize, char, usize), HtmlError> {
    let mut positions = html.match_indices(MARKER);
    let (start, _) = positions.next().ok_or(HtmlError::Invalid)?;
    if positions.next().is_some() {
        return Err(HtmlError::Invalid);
    }
    let value_start = start + MARKER.len();
    let bytes = html.as_bytes();
    if value_start >= bytes.len() {
        return Err(HtmlError::Invalid);
    }
    let quote = bytes[value_start];
    if quote != b'"' && quote != b'\'' {
        return Err(HtmlError::Invalid);
    }
    let inner = value_start + 1;
    let close = html[inner..]
        .find(quote as char)
        .ok_or(HtmlError::Invalid)?;
    Ok((inner, quote as char, inner + close))
}

/// Decode the named (`amp`/`quot`/`apos`/`lt`/`gt`) and numeric (`&#...;` /
/// `&#x...;`) entities Bandcamp emits inside `data-tralbum`. Any other entity
/// or invalid scalar is rejected so we never reinterpret upstream markup.
fn decode_entities(encoded: &str) -> Result<String, HtmlError> {
    let bytes = encoded.as_bytes();
    let mut out = String::with_capacity(encoded.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != b'&' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        let end = encoded[i..].find(';').ok_or(HtmlError::Invalid)? + i;
        let body = &encoded[i + 1..end];
        let ch = match body {
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            "lt" => '<',
            "gt" => '>',
            _ => {
                let num = body.strip_prefix('#').ok_or(HtmlError::Invalid)?;
                let code = if let Some(hex) = num.strip_prefix('x') {
                    u32::from_str_radix(hex, 16)
                } else {
                    num.parse::<u32>()
                }
                .map_err(|_| HtmlError::Invalid)?;
                char::from_u32(code).ok_or(HtmlError::Invalid)?
            }
        };
        out.push(ch);
        if out.len() > DECODED_LIMIT {
            return Err(HtmlError::Invalid);
        }
        i = end + 1;
    }
    Ok(out)
}

fn meta_thumbnail(html: &str) -> Option<String> {
    let mut found = 0usize;
    let mut cursor = 0usize;
    let mut value: Option<String> = None;
    while let Some(rel) = html[cursor..].find("<meta") {
        let start = cursor + rel;
        let Some(close) = html[start..].find('>').map(|e| start + e) else {
            break;
        };
        let tag = &html[start..close];
        if tag.contains("property=\"og:image\"") {
            found += 1;
            if found > 1 {
                return None;
            }
            value = tag
                .split("content=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
                .map(str::to_owned);
        }
        cursor = close + 1;
    }
    (found == 1).then_some(())?;
    value
}

pub fn extract(body: &[u8]) -> Result<TralbumData, HtmlError> {
    if body.len() > bounds::RESPONSE_BODY {
        return Err(HtmlError::Invalid);
    }
    let html = std::str::from_utf8(body).map_err(|_| HtmlError::Invalid)?;
    let (start, _quote, end) = locate_tralbum(html)?;
    let encoded = &html[start..end];
    if encoded.len() > ENCODED_LIMIT {
        return Err(HtmlError::Invalid);
    }
    let json = decode_entities(encoded)?;
    if json.len() > DECODED_LIMIT {
        return Err(HtmlError::Invalid);
    }
    let thumbnail = meta_thumbnail(html);
    Ok(TralbumData { json, thumbnail })
}

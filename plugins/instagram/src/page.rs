use bex_media_url_resolver_v2::{EphemeralLsd, bounds};
use serde_json::Value;
use std::fmt;

const SCRIPT_END: &str = "</script>";
const EQMC_OPEN: &str = "<script id=\"__eqmc\" type=\"application/json\"";
const EQMC_ATTR_MAX: usize = 256;
const RELAY: &str = "<script type=\"application/json\" data-sjs=\"RelayPrefetchedStreamCache\">";
const RELAY_BYTES: usize = 65_536;

#[derive(Debug, Eq, PartialEq)]
pub enum PageError {
    Invalid,
}
impl fmt::Display for PageError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("Instagram page state is unavailable")
    }
}
pub struct PageState {
    lsd: EphemeralLsd,
    relay: Option<String>,
}
impl fmt::Debug for PageState {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("PageState")
            .field("lsd", &"[REDACTED]")
            .field("relay_present", &self.relay.is_some())
            .finish()
    }
}
impl PageState {
    pub fn take(self) -> (EphemeralLsd, Option<String>) {
        (self.lsd, self.relay)
    }
}
fn selected<'a>(html: &'a str, marker: &str) -> Result<Option<&'a str>, PageError> {
    let mut starts = html.match_indices(marker);
    let Some((start, _)) = starts.next() else {
        return Ok(None);
    };
    if starts.next().is_some() {
        return Err(PageError::Invalid);
    }
    let value = &html[start + marker.len()..];
    let end = value.find(SCRIPT_END).ok_or(PageError::Invalid)?;
    Ok(Some(&value[..end]))
}
/// Locates the unique `__eqmc` state script, tolerating optional extra
/// attributes on the opening tag (Instagram now emits a dynamic CSP
/// ` nonce="..."`). The tag must still be unique, close with `>` after only a
/// bounded, single-line attribute run, and carry no nested markup.
fn selected_eqmc(html: &str) -> Result<Option<&str>, PageError> {
    let mut starts = html.match_indices(EQMC_OPEN);
    let Some((start, _)) = starts.next() else {
        return Ok(None);
    };
    if starts.next().is_some() {
        return Err(PageError::Invalid);
    }
    let rest = &html[start + EQMC_OPEN.len()..];
    let close = rest.find('>').ok_or(PageError::Invalid)?;
    let attrs = &rest[..close];
    if close > EQMC_ATTR_MAX || !(attrs.is_empty() || attrs.starts_with(' ')) || attrs.contains('<')
    {
        return Err(PageError::Invalid);
    }
    let value = &rest[close + 1..];
    let end = value.find(SCRIPT_END).ok_or(PageError::Invalid)?;
    Ok(Some(&value[..end]))
}
pub fn extract_page_state(body: &[u8]) -> Result<PageState, PageError> {
    if body.len() > bounds::RESPONSE_BODY {
        return Err(PageError::Invalid);
    }
    let html = std::str::from_utf8(body).map_err(|_| PageError::Invalid)?;
    let eqmc = selected_eqmc(html)?.ok_or(PageError::Invalid)?;
    let token = eqmc
        .strip_prefix("{\"l\":\"")
        .and_then(|value| value.strip_suffix("\"}"))
        .filter(|value| !value.contains(['\"', '\\']))
        .ok_or(PageError::Invalid)?;
    let lsd = EphemeralLsd::new(token.to_owned()).map_err(|_| PageError::Invalid)?;
    let relay = selected(html, RELAY)?
        .map(|value| {
            if value.len() > RELAY_BYTES {
                return Err(PageError::Invalid);
            }
            let parsed: Value = serde_json::from_str(value).map_err(|_| PageError::Invalid)?;
            parsed.as_object().ok_or(PageError::Invalid)?;
            Ok(value.to_owned())
        })
        .transpose()?;
    Ok(PageState { lsd, relay })
}

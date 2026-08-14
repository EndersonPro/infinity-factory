use bex_media_url_resolver_v2::{EphemeralFbDtsg, bounds};
use std::fmt;

/// Ephemeral Tahoe tokens extracted from a Facebook page for the `post-tahoe`
/// fallback (spec Req 5). `fb_dtsg` is wrapped in `EphemeralFbDtsg` (Drop +
/// zeroize, mirroring Instagram's `EphemeralLsd`) so the secret never outlives
/// the `TahoeCall` that consumes it.
pub struct TahoeTokens {
    pub(crate) fb_dtsg: EphemeralFbDtsg,
    pub(crate) pkg_cohort: String,
    pub(crate) client_rev: String,
}
impl fmt::Debug for TahoeTokens {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("TahoeTokens")
            .field("fb_dtsg", &"[REDACTED]")
            .field("pkg_cohort", &self.pkg_cohort)
            .field("client_rev", &self.client_rev)
            .finish()
    }
}

/// Read the quoted string immediately following `opener` in `html`.
fn quoted<'a>(html: &'a str, opener: &str) -> Option<&'a str> {
    let start = html.find(opener)?;
    let value = &html[start + opener.len()..];
    let end = value.find('"')?;
    Some(&value[..end])
}

/// `fb_dtsg` lives near the `DTSGInitialData` beacon: read the next quoted
/// `"token":"..."` after that anchor (`yt_dlp/extractor/facebook.py`).
fn dtsg_token(html: &str) -> Option<&str> {
    let anchor = html.find("\"DTSGInitialData\"")?;
    let rest = &html[anchor..];
    quoted(rest, "\"token\":\"")
}

/// Extract `fb_dtsg`, `__pc` (`pkg_cohort`), and `__rev` (`client_revision`)
/// from the page HTML (spec Req 5). If any token is absent or out of bounds the
/// resolver returns `Unsupported` with no Tahoe call, so this surfaces `None`
/// rather than a partial result.
pub fn extract_tahoe_tokens(html: &str) -> Option<TahoeTokens> {
    let dtsg = dtsg_token(html)?;
    let pkg_cohort = quoted(html, "\"pkg_cohort\":\"")?;
    let client_rev = quoted(html, "\"client_revision\":\"")
        .or_else(|| quoted(html, "\"server_revision\":\""))?;
    let fb_dtsg = EphemeralFbDtsg::new(dtsg.into()).ok()?;
    if pkg_cohort.is_empty()
        || pkg_cohort.len() > bounds::PKG_COHORT
        || client_rev.is_empty()
        || client_rev.len() > bounds::CLIENT_REV
        || !client_rev.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(TahoeTokens {
        fb_dtsg,
        pkg_cohort: pkg_cohort.into(),
        client_rev: client_rev.into(),
    })
}
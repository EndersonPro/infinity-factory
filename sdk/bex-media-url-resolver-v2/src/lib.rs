use std::fmt;
use url::Url;
use zeroize::Zeroize;

wit_bindgen::generate!({
    world: "media-url-resolver",
    path: "../../wit/media-url-resolver-v2/wit",
    pub_export_macro: true,
});

pub use component::media_url_resolver::https_client::{
    GetRequest, Header, HttpsError, HttpsResponse, PublicGraphqlRequest, TahoeRequest,
};
pub use exports::component::media_url_resolver::resolver::{
    Candidate, ContextEntry, Deferred, Guest as ResolverGuest, Header as ResolverHeader,
    MediaStream, Metadata, MuxContainer, MuxPlan, QualityPreference, Resolution, ResolveIntent,
    ResolveRequest, ResolveResponse, ResolverError, ResolverErrorKind, SeparatedStreams,
    Unsupported,
};
mod mock;
mod resolver;
pub use mock::{
    ExpectedCall, MockHttpsClient, Observation, PublicGraphqlExpectation, TahoeExpectation,
};
pub use resolver::{
    bounds as resolver_bounds, validate_request as validate_resolver_request,
    validate_response as validate_resolver_response,
};
#[macro_export]
macro_rules! export_resolver_v2 {
    ($component:ident) => {
        $crate::export!($component with_types_in $crate);
    };
}
pub mod bounds {
    pub const URL: usize = 2_048;
    pub const GET_HEADERS: usize = 8;
    pub const GET_HEADER_NAME: usize = 64;
    pub const GET_HEADER_VALUE: usize = 1_024;
    pub const GET_HEADERS_COMBINED: usize = 4_096;
    pub const LSD: usize = 256;
    pub const FRIENDLY_NAME: usize = 128;
    pub const DOC_ID: usize = 64;
    pub const VARIABLES: usize = 32_768;
    pub const FORM_BODY: usize = 65_536;
    pub const FB_DTSG: usize = 256;
    pub const PKG_COHORT: usize = 128;
    pub const CLIENT_REV: usize = 64;
    pub const RESPONSE_HEADERS: usize = 16;
    pub const RESPONSE_HEADER_NAME: usize = 64;
    pub const RESPONSE_HEADER_VALUE: usize = 4_096;
    pub const RESPONSE_HEADERS_COMBINED: usize = 16_384;
    pub const RESPONSE_BODY: usize = 4_194_304;
}
fn error(kind: ResolverErrorKind, message: &str) -> ResolverError {
    ResolverError {
        kind,
        retryable: false,
        safe_message: message.into(),
    }
}
fn request_error() -> ResolverError {
    error(
        ResolverErrorKind::InvalidInput,
        "HTTPS request violates compatibility policy",
    )
}
fn response_error() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        "HTTPS response violates compatibility policy",
    )
}
fn instagram_authority(authority: &str) -> bool {
    matches!(authority, "instagram.com" | "www.instagram.com")
}
fn bandcamp_artist(authority: &str) -> Option<&str> {
    authority.strip_suffix(".bandcamp.com").filter(|label| {
        !label.is_empty()
            && *label != "www"
            && label.len() <= 63
            && label
                .bytes()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    })
}
fn youtube_authority(authority: &str) -> bool {
    matches!(
        authority,
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "youtu.be"
    )
}
/// The single X syndication host, matched exactly.
///
/// No suffix or wildcard form: `compatibility/v2/abi-identity-vectors.json`
/// names `wildcard-host` and `deceptive-host` as invalid ABI shapes, and a
/// suffix match here would admit `cdn.syndication.twimg.com.example.org`.
fn x_syndication_authority(authority: &str) -> bool {
    authority == "cdn.syndication.twimg.com"
}
/// The single canonical TikTok public-video host, matched exactly.
///
/// No suffix or wildcard form: `compatibility/v2/abi-identity-vectors.json`
/// names `wildcard-host` and `deceptive-host` as invalid ABI shapes, and a
/// suffix match here would admit `www.tiktok.com.evil.com`.
fn tiktok_authority(authority: &str) -> bool {
    authority == "www.tiktok.com"
}
/// The two canonical public-page authorities the host admits: `www` and the
/// `web` mobile-web hop Facebook's edge bounces a logged-out GET through
/// (verified live — see `rust/src/host.rs::is_canonical_facebook_get_url` in
/// the host repository for the paired admission logic).
///
/// Matching the literal set protects the shared SDK gate from accidentally
/// granting the apex, mobile hosts, or suffix look-alikes.
fn facebook_authority(authority: &str) -> bool {
    authority == "www.facebook.com" || authority == "web.facebook.com"
}
fn parsed_https(value: &str) -> Option<Url> {
    if value.len() > bounds::URL || !value.starts_with("https://") {
        return None;
    }
    let authority = value[8..].split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains(['@', ':']) {
        return None;
    }
    let parsed = Url::parse(value).ok()?;
    (parsed.scheme() == "https"
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
        && parsed.fragment().is_none()
        && parsed.host_str() == Some(authority)
        && (instagram_authority(authority)
            || bandcamp_artist(authority).is_some()
            || youtube_authority(authority)
            || x_syndication_authority(authority)
            || tiktok_authority(authority)
            || facebook_authority(authority)))
    .then_some(parsed)
}
/// Exact `/watch?v=<id>` query shape: a single `v` parameter carrying an
/// 11-character YouTube video id, and nothing else (no other params, no
/// repeated `v`, no trailing junk after the id).
fn valid_watch_query(query: &str) -> bool {
    query.strip_prefix("v=").is_some_and(|id| {
        id.len() == 11
            && id
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-'))
    })
}
/// Exact `?id={digits}&token={token}` shape: two pairs, in that order, and
/// nothing else.
///
/// A total comparison rather than a parse over query pairs. A parse has to
/// decide what a repeated `id` means, and every such decision is somewhere a
/// second one can hide; this shape has nowhere to put it.
fn valid_syndication_query(query: &str) -> bool {
    let Some((id, token)) = query.split_once('&') else {
        return false;
    };
    let (Some(id), Some(token)) = (id.strip_prefix("id="), token.strip_prefix("token=")) else {
        return false;
    };
    valid_syndication_id(id) && valid_syndication_token(token)
}
/// 1-19 ASCII digits, no leading zero.
///
/// Bounded at 19 rather than 20 so every admitted id parses into a `u64`: a
/// 20-digit value may exceed `u64::MAX`, and 19 never does.
fn valid_syndication_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 19
        && !id.starts_with('0')
        && id.bytes().all(|value| value.is_ascii_digit())
}
/// 1-16 characters drawn from `1-9a-z`.
///
/// `0` is excluded deliberately: the token derivation strips `0` and `.` from
/// its base-36 output, so a token carrying a zero is a string the guest cannot
/// produce, and admitting it would widen the gate past what it can emit.
fn valid_syndication_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 16
        && token
            .bytes()
            .all(|value| matches!(value, b'1'..=b'9') || value.is_ascii_lowercase())
}
/// Exact player-JS asset path shape: `/s/player/{hash}/{variant}/{locale}/base.js`.
/// This is the only non-watch GET the plugin issues, needed to fetch YouTube's
/// player JS for signature-cipher decoding.
fn valid_player_js_path(parts: &[&str]) -> bool {
    parts.len() == 7
        && parts[0].is_empty()
        && parts[1] == "s"
        && parts[2] == "player"
        && !parts[3].is_empty()
        && parts[3].len() <= 40
        && parts[3].bytes().all(|value| value.is_ascii_alphanumeric())
        && !parts[4].is_empty()
        && parts[4].len() <= 40
        && parts[4]
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'.'))
        && !parts[5].is_empty()
        && parts[5].len() <= 16
        && parts[5]
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'_')
        && parts[6] == "base.js"
}
/// TikTok `@{user}` label: 1-64 bytes from `[A-Za-z0-9._-]`. The literal `_`
/// is a canonical user segment (yt-dlp's `_create_url` fallback at
/// `yt_dlp/extractor/tiktok.py:106-108`), not a sentinel; it is admitted by
/// this character set.
fn valid_tiktok_user_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
}
/// TikTok video id: 1-19 ASCII digits.
fn valid_tiktok_video_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 19 && id.bytes().all(|value| value.is_ascii_digit())
}
/// Facebook user label: 1-64 bytes from `[A-Za-z0-9._-]`.
fn valid_facebook_user_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'_' | b'-'))
}
/// Facebook public-video ids are decimal values or canonical `pfbid` values.
fn valid_facebook_video_id(id: &str) -> bool {
    (!id.is_empty() && id.len() <= 64 && id.bytes().all(|value| value.is_ascii_digit()))
        || (id.len() >= 6
            && id.len() <= 64
            && id.starts_with("pfbid")
            && id[5..]
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-')))
}
/// Exact Facebook watch query shape: one `v` parameter and no extras.
fn valid_facebook_watch_query(query: &str) -> bool {
    query
        .strip_prefix("v=")
        .is_some_and(valid_facebook_video_id)
}
fn valid_facebook_get_url(url: &Url) -> bool {
    let parts: Vec<&str> = url.path().split('/').collect();
    match url.query() {
        Some(query) => {
            parts.len() == 3
                && parts[0].is_empty()
                && parts[1] == "watch"
                && parts[2].is_empty()
                && valid_facebook_watch_query(query)
        }
        None => match parts.as_slice() {
            ["", user, "videos", id, ""]
            | ["", user, "reels", id, ""]
            | ["", user, "reel", id, ""] => {
                valid_facebook_user_label(user) && valid_facebook_video_id(id)
            }
            ["", "reel", id, ""] => valid_facebook_video_id(id),
            _ => false,
        },
    }
}
fn valid_get_url(value: &str) -> bool {
    let Some(url) = parsed_https(value) else {
        return false;
    };
    let authority = url.host_str().unwrap_or("");
    let parts: Vec<&str> = url.path().split('/').collect();
    if instagram_authority(authority) {
        url.query().is_none()
            && parts.len() == 4
            && parts[0].is_empty()
            && matches!(parts[1], "p" | "reel" | "reels" | "tv")
            && !parts[2].is_empty()
            && parts[2].len() <= 64
            && parts[2]
                .bytes()
                .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-'))
            && parts[3].is_empty()
    } else if bandcamp_artist(authority).is_some() {
        url.query().is_none()
            && parts.len() == 3
            && parts[0].is_empty()
            && parts[1] == "track"
            && !parts[2].is_empty()
            && parts[2].len() <= 128
            && parts[2]
                .bytes()
                .all(|value| value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'-')
    } else if youtube_authority(authority) {
        match url.query() {
            Some(query) => {
                parts.len() == 2
                    && parts[0].is_empty()
                    && parts[1] == "watch"
                    && valid_watch_query(query)
            }
            None => valid_player_js_path(&parts),
        }
    } else if x_syndication_authority(authority) {
        parts.len() == 2
            && parts[0].is_empty()
            && parts[1] == "tweet-result"
            && url.query().is_some_and(valid_syndication_query)
    } else if tiktok_authority(authority) {
        url.query().is_none()
            && parts.len() == 4
            && parts[0].is_empty()
            && parts[1].starts_with('@')
            && parts[1].len() >= 2
            && parts[1].len() <= 65
            && valid_tiktok_user_label(&parts[1][1..])
            && parts[2] == "video"
            && valid_tiktok_video_id(parts[3])
    } else if facebook_authority(authority) {
        valid_facebook_get_url(&url)
    } else {
        false
    }
}
fn valid_tahoe_url(value: &str) -> bool {
    if value.len() > bounds::URL || !value.starts_with("https://") {
        return false;
    }
    let Ok(parsed) = Url::parse(value) else {
        return false;
    };
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.fragment().is_some()
        || parsed.host_str() != Some("www.facebook.com")
    {
        return false;
    }
    let parts: Vec<&str> = parsed.path().split('/').collect();
    parts.len() == 6
        && parts[0].is_empty()
        && parts[1] == "video"
        && parts[2] == "tahoe"
        && parts[3] == "async"
        && !parts[4].is_empty()
        && parts[4].len() <= 64
        && parts[4]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        && parts[5].is_empty()
        && parsed.query().is_none()
}
fn valid_headers(headers: &[Header], response: bool) -> bool {
    let (count, name, value, combined) = if response {
        (
            bounds::RESPONSE_HEADERS,
            bounds::RESPONSE_HEADER_NAME,
            bounds::RESPONSE_HEADER_VALUE,
            bounds::RESPONSE_HEADERS_COMBINED,
        )
    } else {
        (
            bounds::GET_HEADERS,
            bounds::GET_HEADER_NAME,
            bounds::GET_HEADER_VALUE,
            bounds::GET_HEADERS_COMBINED,
        )
    };
    if headers.len() > count
        || headers
            .iter()
            .map(|item| item.name.len() + item.value.len())
            .sum::<usize>()
            > combined
    {
        return false;
    }
    let mut names = Vec::with_capacity(headers.len());
    for header in headers {
        if header.name.is_empty()
            || header.name.len() > name
            || header.value.len() > value
            || header.value.bytes().any(|byte| byte < 32 || byte == 127)
            || header
                .name
                .bytes()
                .any(|byte| !byte.is_ascii_lowercase() && byte != b'-')
        {
            return false;
        }
        let allowed = if response {
            matches!(
                header.name.as_str(),
                "cache-control"
                    | "content-length"
                    | "content-type"
                    | "etag"
                    | "last-modified"
                    | "retry-after"
            )
        } else {
            matches!(header.name.as_str(), "accept" | "accept-language")
        };
        if !allowed {
            return false;
        }
        names.push(header.name.as_str());
    }
    names.sort_unstable();
    !names.windows(2).any(|pair| pair[0] == pair[1])
}
pub fn validate_get_request(request: &GetRequest) -> Result<(), ResolverError> {
    (valid_get_url(&request.url) && valid_headers(&request.headers, false))
        .then_some(())
        .ok_or_else(request_error)
}
pub fn validate_public_graphql_request(
    request: &PublicGraphqlRequest,
) -> Result<(), ResolverError> {
    let variables = serde_json::from_str::<serde_json::Value>(&request.variables).ok();
    let encoded = |value: &str| {
        value
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
                    1
                } else {
                    3
                }
            })
            .sum::<usize>()
    };
    let form_size = 64
        + encoded(&request.lsd)
        + encoded(&request.friendly_name)
        + encoded(&request.variables)
        + encoded(&request.doc_id);
    (request.url == "https://www.instagram.com/api/graphql"
        && request.lsd.len() <= bounds::LSD
        && !request.lsd.is_empty()
        && request.lsd.bytes().all(|byte| byte.is_ascii_graphic())
        && !request.friendly_name.is_empty()
        && request.friendly_name.len() <= bounds::FRIENDLY_NAME
        && !request.doc_id.is_empty()
        && request.doc_id.len() <= bounds::DOC_ID
        && request.doc_id.bytes().all(|byte| byte.is_ascii_digit())
        && request.variables.len() <= bounds::VARIABLES
        && variables.is_some_and(|value| value.is_object())
        && form_size <= bounds::FORM_BODY)
        .then_some(())
        .ok_or_else(request_error)
}
pub fn validate_tahoe_request(request: &TahoeRequest) -> Result<(), ResolverError> {
    (valid_tahoe_url(&request.url)
        && !request.fb_dtsg.is_empty()
        && request.fb_dtsg.len() <= bounds::FB_DTSG
        && request.fb_dtsg.bytes().all(|byte| byte.is_ascii_graphic())
        && !request.pkg_cohort.is_empty()
        && request.pkg_cohort.len() <= bounds::PKG_COHORT
        && request
            .pkg_cohort
            .bytes()
            .all(|byte| byte.is_ascii_graphic())
        && !request.client_rev.is_empty()
        && request.client_rev.len() <= bounds::CLIENT_REV
        && request.client_rev.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(())
    .ok_or_else(request_error)
}
pub fn validate_https_response(response: &HttpsResponse) -> Result<(), ResolverError> {
    ((100..=599).contains(&response.status)
        && (valid_get_url(&response.final_url)
            || response.final_url == "https://www.instagram.com/api/graphql"
            || valid_tahoe_url(&response.final_url))
        && valid_headers(&response.headers, true)
        && response.body.len() <= bounds::RESPONSE_BODY)
        .then_some(())
        .ok_or_else(response_error)
}
pub struct EphemeralLsd(String);
impl EphemeralLsd {
    pub fn new(value: String) -> Result<Self, ResolverError> {
        (!value.is_empty()
            && value.len() <= bounds::LSD
            && value.bytes().all(|byte| byte.is_ascii_graphic()))
        .then_some(Self(value))
        .ok_or_else(request_error)
    }
    fn take(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}
impl Drop for EphemeralLsd {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl fmt::Debug for EphemeralLsd {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("[REDACTED]")
    }
}
impl fmt::Display for EphemeralLsd {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("[REDACTED]")
    }
}
pub struct EphemeralFbDtsg(String);
impl EphemeralFbDtsg {
    pub fn new(value: String) -> Result<Self, ResolverError> {
        (!value.is_empty()
            && value.len() <= bounds::FB_DTSG
            && value.bytes().all(|byte| byte.is_ascii_graphic()))
        .then_some(Self(value))
        .ok_or_else(request_error)
    }
    fn take(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}
impl Drop for EphemeralFbDtsg {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl fmt::Debug for EphemeralFbDtsg {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("[REDACTED]")
    }
}
impl fmt::Display for EphemeralFbDtsg {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str("[REDACTED]")
    }
}
pub struct PublicGraphqlCall(PublicGraphqlRequest);
impl Drop for PublicGraphqlCall {
    fn drop(&mut self) {
        self.0.lsd.zeroize();
    }
}
pub fn build_graphql_request(
    url: &str,
    lsd: EphemeralLsd,
    friendly_name: &str,
    doc_id: &str,
    variables: &str,
) -> Result<PublicGraphqlCall, ResolverError> {
    let request = PublicGraphqlCall(PublicGraphqlRequest {
        url: url.into(),
        lsd: lsd.take(),
        friendly_name: friendly_name.into(),
        doc_id: doc_id.into(),
        variables: variables.into(),
    });
    validate_public_graphql_request(&request.0)?;
    Ok(request)
}
pub struct TahoeCall(TahoeRequest);
impl Drop for TahoeCall {
    fn drop(&mut self) {
        self.0.fb_dtsg.zeroize();
    }
}
pub fn build_tahoe_request(
    url: &str,
    fb_dtsg: EphemeralFbDtsg,
    pkg_cohort: &str,
    client_rev: &str,
) -> Result<TahoeCall, ResolverError> {
    let request = TahoeCall(TahoeRequest {
        url: url.into(),
        fb_dtsg: fb_dtsg.take(),
        pkg_cohort: pkg_cohort.into(),
        client_rev: client_rev.into(),
    });
    validate_tahoe_request(&request.0)?;
    Ok(request)
}
pub trait HttpsClient {
    fn get(&mut self, request: GetRequest) -> Result<HttpsResponse, HttpsError>;
    fn post_public_graphql(
        &mut self,
        request: PublicGraphqlCall,
    ) -> Result<HttpsResponse, HttpsError>;
    fn post_tahoe(&mut self, request: TahoeCall) -> Result<HttpsResponse, HttpsError>;
}
pub struct WasmHttpsClient;
#[cfg(target_arch = "wasm32")]
fn checked_response(
    result: Result<HttpsResponse, HttpsError>,
) -> Result<HttpsResponse, HttpsError> {
    let response = result?;
    validate_https_response(&response).map_err(|_| HttpsError::MalformedUpstream)?;
    Ok(response)
}
#[cfg(target_arch = "wasm32")]
impl HttpsClient for WasmHttpsClient {
    fn get(&mut self, request: GetRequest) -> Result<HttpsResponse, HttpsError> {
        validate_get_request(&request).map_err(|_| HttpsError::InvalidRequest)?;
        checked_response(component::media_url_resolver::https_client::get(&request))
    }
    fn post_public_graphql(
        &mut self,
        request: PublicGraphqlCall,
    ) -> Result<HttpsResponse, HttpsError> {
        validate_public_graphql_request(&request.0).map_err(|_| HttpsError::InvalidRequest)?;
        checked_response(
            component::media_url_resolver::https_client::post_public_graphql(&request.0),
        )
    }
    fn post_tahoe(&mut self, request: TahoeCall) -> Result<HttpsResponse, HttpsError> {
        validate_tahoe_request(&request.0).map_err(|_| HttpsError::InvalidRequest)?;
        checked_response(component::media_url_resolver::https_client::post_tahoe(
            &request.0,
        ))
    }
}

#[cfg(test)]
mod tahoe_tests {
    use super::*;

    #[test]
    fn ephemeral_fb_dtsg_constructs_valid() {
        let value = EphemeralFbDtsg::new("valid_fb_dtsg_token".into());
        assert!(value.is_ok());
        let value = value.unwrap();
        assert_eq!(value.0, "valid_fb_dtsg_token");
    }

    #[test]
    fn ephemeral_fb_dtsg_rejects_invalid() {
        assert!(EphemeralFbDtsg::new(String::new()).is_err());
        let oversize = "x".repeat(256 + 1);
        assert!(EphemeralFbDtsg::new(oversize).is_err());
        assert!(EphemeralFbDtsg::new("bad\0token".into()).is_err());
        assert!(EphemeralFbDtsg::new("has space".into()).is_err());
    }

    #[test]
    fn ephemeral_fb_dtsg_at_bound_accepted() {
        let at_bound = "x".repeat(256);
        assert!(EphemeralFbDtsg::new(at_bound).is_ok());
    }

    #[test]
    fn ephemeral_fb_dtsg_has_drop_impl() {
        assert!(std::mem::needs_drop::<EphemeralFbDtsg>());
    }

    #[test]
    fn ephemeral_fb_dtsg_zeroize_zeros_buffer_bytes() {
        let secret = "secrettoken".to_string();
        let ptr = secret.as_ptr();
        let len = secret.len();
        let mut value = EphemeralFbDtsg::new(secret).unwrap();
        value.0.zeroize();
        assert!(value.0.is_empty());
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, len);
            assert!(
                bytes.iter().all(|&b| b == 0),
                "fb_dtsg buffer bytes were not zeroized"
            );
        }
    }

    #[test]
    fn ephemeral_fb_dtsg_take_extracts_secret_and_drop_is_safe() {
        let value = EphemeralFbDtsg::new("takethissecret".into()).unwrap();
        let extracted = value.take();
        assert_eq!(extracted, "takethissecret");
    }

    #[test]
    fn ephemeral_fb_dtsg_debug_and_display_redact() {
        let value = EphemeralFbDtsg::new("SECRETVALUE".into()).unwrap();
        let debug = format!("{:?}", value);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains("SECRETVALUE"));
        let display = format!("{}", value);
        assert!(display.contains("[REDACTED]"));
        assert!(!display.contains("SECRETVALUE"));
    }

    #[test]
    fn tahoe_call_has_drop_impl() {
        assert!(std::mem::needs_drop::<TahoeCall>());
    }

    #[test]
    fn tahoe_call_zeroize_zeros_fb_dtsg_buffer() {
        let mut call = build_tahoe_request(
            "https://www.facebook.com/video/tahoe/async/10107927396957931/",
            EphemeralFbDtsg::new("sensitive_fb_dtsg".into()).unwrap(),
            "PHASED:DEFAULT",
            "123456",
        )
        .unwrap();
        let ptr = call.0.fb_dtsg.as_ptr();
        let len = call.0.fb_dtsg.len();
        call.0.fb_dtsg.zeroize();
        assert!(call.0.fb_dtsg.is_empty());
        unsafe {
            let bytes = std::slice::from_raw_parts(ptr, len);
            assert!(
                bytes.iter().all(|&b| b == 0),
                "TahoeCall fb_dtsg buffer bytes were not zeroized"
            );
        }
    }
}

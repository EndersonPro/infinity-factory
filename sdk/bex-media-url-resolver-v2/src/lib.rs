use std::fmt;
use url::Url;
use zeroize::Zeroize;

wit_bindgen::generate!({
    world: "media-url-resolver",
    path: "../../wit/media-url-resolver-v2/wit",
    pub_export_macro: true,
});

pub use component::media_url_resolver::https_client::{
    GetRequest, Header, HttpsError, HttpsResponse, PublicGraphqlRequest,
};
pub use exports::component::media_url_resolver::resolver::{
    Candidate, Deferred, Guest as ResolverGuest, MediaStream, Metadata, MuxContainer, MuxPlan,
    QualityPreference, Resolution, ResolveIntent, ResolveRequest, ResolveResponse, ResolverError,
    ResolverErrorKind, SeparatedStreams, Unsupported,
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
        && matches!(authority, "instagram.com" | "www.instagram.com"))
    .then_some(parsed)
}
fn valid_get_url(value: &str) -> bool {
    let Some(url) = parsed_https(value) else {
        return false;
    };
    if url.query().is_some() {
        return false;
    }
    let parts: Vec<_> = url.path().split('/').collect();
    parts.len() == 4
        && parts[0].is_empty()
        && matches!(parts[1], "p" | "reel" | "reels" | "tv")
        && !parts[2].is_empty()
        && parts[2].len() <= 64
        && parts[2]
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'_' | b'-'))
        && parts[3].is_empty()
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
pub fn validate_https_response(response: &HttpsResponse) -> Result<(), ResolverError> {
    ((100..=599).contains(&response.status)
        && (valid_get_url(&response.final_url)
            || response.final_url == "https://www.instagram.com/api/graphql")
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
pub trait HttpsClient {
    fn get(&mut self, request: GetRequest) -> Result<HttpsResponse, HttpsError>;
    fn post_public_graphql(
        &mut self,
        request: PublicGraphqlCall,
    ) -> Result<HttpsResponse, HttpsError>;
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
}

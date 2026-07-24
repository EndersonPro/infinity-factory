use super::{
    MediaStream, Resolution, ResolveRequest, ResolveResponse, ResolverError, ResolverErrorKind,
};
use url::Url;

pub mod bounds {
    pub const URL: usize = 2_048;
    pub const CORRELATION_ID: usize = 64;
    pub const QUALITY_PREFERENCES: usize = 8;
    pub const CONTEXT_ENTRIES: usize = 16;
    pub const CONTEXT_KEY: usize = 64;
    pub const CONTEXT_VALUE: usize = 256;
    pub const CONTEXT_COMBINED: usize = 4_096;
    pub const CANDIDATES: usize = 16;
    pub const CANDIDATE_ID: usize = 64;
    pub const HEADERS: usize = 16;
    pub const HEADER_NAME: usize = 64;
    pub const HEADER_VALUE: usize = 1_024;
    pub const HEADERS_COMBINED: usize = 8_192;
    pub const FORMAT: usize = 32;
    pub const MIME: usize = 128;
    pub const LABEL: usize = 64;
    pub const TITLE: usize = 256;
    pub const AUTHOR: usize = 128;
    pub const REASON: usize = 256;
    pub const RETRY_AFTER: u32 = 86_400;
}
fn failure(kind: ResolverErrorKind, message: &str) -> ResolverError {
    ResolverError {
        kind,
        retryable: false,
        safe_message: message.into(),
    }
}
fn within(value: &Option<String>, limit: usize) -> bool {
    value
        .as_ref()
        .is_none_or(|item| item.len() <= limit && !has_control(item))
}
fn has_control(value: &str) -> bool {
    value.bytes().any(|byte| byte < 32 || byte == 127)
}
fn safe_https(value: &str) -> bool {
    if value.is_empty() || value.len() > bounds::URL {
        return false;
    }
    Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && url.fragment().is_none()
    })
}
pub fn validate_request(request: &ResolveRequest) -> Result<(), ResolverError> {
    let mut keys: Vec<_> = request
        .client_context
        .iter()
        .map(|entry| &entry.key)
        .collect();
    keys.sort_unstable();
    let invalid = !super::valid_get_url(&request.source_url)
        || request.correlation_id.is_empty()
        || request.correlation_id.len() > bounds::CORRELATION_ID
        || has_control(&request.correlation_id)
        || request.quality_preferences.len() > bounds::QUALITY_PREFERENCES
        || request.client_context.len() > bounds::CONTEXT_ENTRIES
        || request
            .client_context
            .iter()
            .map(|entry| entry.key.len() + entry.value.len())
            .sum::<usize>()
            > bounds::CONTEXT_COMBINED
        || keys.windows(2).any(|pair| pair[0] == pair[1])
        || request.client_context.iter().any(|entry| {
            entry.key.is_empty()
                || entry.key.len() > bounds::CONTEXT_KEY
                || entry.value.len() > bounds::CONTEXT_VALUE
                || has_control(&entry.key)
                || has_control(&entry.value)
        })
        || request.quality_preferences.iter().any(|quality| {
            !within(&quality.container, bounds::FORMAT)
                || !within(&quality.mime_type, bounds::MIME)
                || !within(&quality.quality_label, bounds::LABEL)
        });
    (!invalid).then_some(()).ok_or_else(|| {
        failure(
            ResolverErrorKind::InvalidInput,
            "resolver request violates compatibility limits",
        )
    })
}
fn validate_stream(stream: &MediaStream) -> bool {
    let mut names: Vec<_> = stream
        .headers
        .iter()
        .map(|header| header.name.to_ascii_lowercase())
        .collect();
    names.sort_unstable();
    safe_https(&stream.url)
        && within(&stream.format, bounds::FORMAT)
        && within(&stream.mime_type, bounds::MIME)
        && within(&stream.quality_label, bounds::LABEL)
        && within(&stream.codecs, bounds::MIME)
        && stream.headers.len() <= bounds::HEADERS
        && stream
            .headers
            .iter()
            .map(|header| header.name.len() + header.value.len())
            .sum::<usize>()
            <= bounds::HEADERS_COMBINED
        && !names.windows(2).any(|pair| pair[0] == pair[1])
        && stream.headers.iter().all(|header| {
            let name = header.name.to_ascii_lowercase();
            !header.name.is_empty()
                && header.name.len() <= bounds::HEADER_NAME
                && header.value.len() <= bounds::HEADER_VALUE
                && !has_control(&header.name)
                && !has_control(&header.value)
                && !matches!(
                    name.as_str(),
                    "authorization"
                        | "proxy-authorization"
                        | "cookie"
                        | "set-cookie"
                        | "host"
                        | "forwarded"
                        | "x-forwarded-for"
                        | "x-forwarded-host"
                        | "x-forwarded-proto"
                        | "connection"
                        | "content-length"
                        | "transfer-encoding"
                )
        })
}
pub fn validate_response(response: &ResolveResponse) -> Result<(), ResolverError> {
    let metadata_valid = response.metadata.as_ref().is_none_or(|metadata| {
        within(&metadata.title, bounds::TITLE)
            && within(&metadata.author, bounds::AUTHOR)
            && metadata
                .thumbnail_url
                .as_ref()
                .is_none_or(|url| safe_https(url))
    });
    let resolution_valid = match &response.resolution {
        Resolution::Direct(stream) => validate_stream(stream),
        Resolution::Candidates(items) => {
            let mut ids: Vec<_> = items.iter().map(|item| &item.id).collect();
            ids.sort_unstable();
            !items.is_empty()
                && items.len() <= bounds::CANDIDATES
                && !ids.windows(2).any(|pair| pair[0] == pair[1])
                && items.iter().all(|item| {
                    !item.id.is_empty()
                        && item.id.len() <= bounds::CANDIDATE_ID
                        && !has_control(&item.id)
                        && validate_stream(&item.stream)
                })
        }
        Resolution::Separated(value) => {
            validate_stream(&value.audio) && validate_stream(&value.video)
        }
        Resolution::Unsupported(value) => {
            value.reason.len() <= bounds::REASON && !has_control(&value.reason)
        }
        Resolution::Deferred(value) => {
            value.reason.len() <= bounds::REASON
                && !has_control(&value.reason)
                && value
                    .retry_after_seconds
                    .is_none_or(|seconds| seconds <= bounds::RETRY_AFTER)
        }
    };
    (metadata_valid && resolution_valid)
        .then_some(())
        .ok_or_else(|| {
            failure(
                ResolverErrorKind::MalformedResponse,
                "resolver response violates compatibility limits",
            )
        })
}

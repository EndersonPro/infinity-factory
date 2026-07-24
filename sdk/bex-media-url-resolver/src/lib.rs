wit_bindgen::generate!({
    world: "media-url-resolver",
    path: "../../wit/media-url-resolver/wit",
    pub_export_macro: true,
});

pub use exports::component::media_url_resolver::resolver::{
    Candidate, Deferred, Guest as ResolverGuest, Header, MediaStream, Metadata, MuxContainer,
    MuxPlan, QualityPreference, Resolution, ResolveIntent, ResolveRequest, ResolveResponse,
    ResolverError, ResolverErrorKind, SeparatedStreams, Unsupported,
};

#[macro_export]
macro_rules! export_resolver {
    ($component:ident) => {
        $crate::export!($component with_types_in $crate);
    };
}

pub mod bounds {
    pub const URL_BYTES: usize = 2_048;
    pub const CORRELATION_ID_BYTES: usize = 64;
    pub const QUALITY_PREFERENCES: usize = 8;
    pub const CONTEXT_ENTRIES: usize = 16;
    pub const CONTEXT_KEY_BYTES: usize = 64;
    pub const CONTEXT_VALUE_BYTES: usize = 256;
    pub const CANDIDATES: usize = 16;
    pub const CANDIDATE_ID_BYTES: usize = 64;
    pub const HEADERS_PER_STREAM: usize = 16;
    pub const HEADER_NAME_BYTES: usize = 64;
    pub const HEADER_VALUE_BYTES: usize = 1_024;
    pub const FORMAT_BYTES: usize = 32;
    pub const MIME_BYTES: usize = 128;
    pub const LABEL_BYTES: usize = 64;
    pub const REASON_BYTES: usize = 256;
}

fn within(value: &Option<String>, limit: usize) -> bool {
    value.as_ref().is_none_or(|value| value.len() <= limit)
}

fn invalid(message: &str) -> ResolverError {
    ResolverError {
        kind: ResolverErrorKind::InvalidInput,
        retryable: false,
        safe_message: message.into(),
    }
}

pub fn validate_request(request: &ResolveRequest) -> Result<(), ResolverError> {
    let mut keys: Vec<_> = request
        .client_context
        .iter()
        .map(|entry| &entry.key)
        .collect();
    keys.sort_unstable();
    let duplicate_keys = keys.windows(2).any(|pair| pair[0] == pair[1]);
    let invalid_request = request.source_url.len() > bounds::URL_BYTES
        || request.correlation_id.len() > bounds::CORRELATION_ID_BYTES
        || request.quality_preferences.len() > bounds::QUALITY_PREFERENCES
        || request.client_context.len() > bounds::CONTEXT_ENTRIES
        || duplicate_keys
        || request.client_context.iter().any(|entry| {
            entry.key.len() > bounds::CONTEXT_KEY_BYTES
                || entry.value.len() > bounds::CONTEXT_VALUE_BYTES
        })
        || request.quality_preferences.iter().any(|quality| {
            !within(&quality.container, bounds::FORMAT_BYTES)
                || !within(&quality.mime_type, bounds::MIME_BYTES)
                || !within(&quality.quality_label, bounds::LABEL_BYTES)
        });
    (!invalid_request)
        .then_some(())
        .ok_or_else(|| invalid("request exceeds compatibility limits"))
}

pub fn validate_stream(stream: &MediaStream) -> Result<(), ResolverError> {
    let mut names: Vec<_> = stream
        .headers
        .iter()
        .map(|header| header.name.to_ascii_lowercase())
        .collect();
    names.sort_unstable();
    let invalid_stream = stream.url.len() > bounds::URL_BYTES
        || !stream.url.starts_with("https://")
        || !within(&stream.format, bounds::FORMAT_BYTES)
        || !within(&stream.mime_type, bounds::MIME_BYTES)
        || !within(&stream.quality_label, bounds::LABEL_BYTES)
        || !within(&stream.codecs, bounds::MIME_BYTES)
        || stream.headers.len() > bounds::HEADERS_PER_STREAM
        || names.windows(2).any(|pair| pair[0] == pair[1])
        || stream.headers.iter().any(|header| {
            header.name.len() > bounds::HEADER_NAME_BYTES
                || header.value.len() > bounds::HEADER_VALUE_BYTES
        });
    (!invalid_stream)
        .then_some(())
        .ok_or_else(|| invalid("stream violates compatibility limits"))
}

pub fn validate_response(response: &ResolveResponse) -> Result<(), ResolverError> {
    match &response.resolution {
        Resolution::Direct(stream) => validate_stream(stream),
        Resolution::Candidates(items) => {
            if items.is_empty() || items.len() > bounds::CANDIDATES {
                return Err(invalid("candidate count violates compatibility limits"));
            }
            let mut ids: Vec<_> = items.iter().map(|item| &item.id).collect();
            let ordered = ids.windows(2).all(|pair| pair[0] <= pair[1]);
            ids.sort_unstable();
            if !ordered
                || ids.windows(2).any(|pair| pair[0] == pair[1])
                || items
                    .iter()
                    .any(|item| item.id.len() > bounds::CANDIDATE_ID_BYTES)
            {
                return Err(invalid("candidate identity is ambiguous"));
            }
            items
                .iter()
                .try_for_each(|item| validate_stream(&item.stream))
        }
        Resolution::Separated(value) => {
            validate_stream(&value.audio)?;
            validate_stream(&value.video)
        }
        Resolution::Unsupported(value) if value.reason.len() <= bounds::REASON_BYTES => Ok(()),
        Resolution::Deferred(value) if value.reason.len() <= bounds::REASON_BYTES => Ok(()),
        _ => Err(invalid("response exceeds compatibility limits")),
    }
}

use bex_media_url_resolver_v2::{HttpsError, ResolverError, ResolverErrorKind};

fn error(kind: ResolverErrorKind, retryable: bool, message: &str) -> ResolverError {
    ResolverError {
        kind,
        retryable,
        safe_message: message.into(),
    }
}
/// `InvalidInput` — the source URL is not a canonical TikTok video page
/// (spec Req 1).
pub(crate) fn invalid_input() -> ResolverError {
    error(
        ResolverErrorKind::InvalidInput,
        false,
        "unsupported public TikTok input",
    )
}
/// `MalformedResponse` — the response body is not the expected universal-data
/// block (spec Req 3 absent/malformed).
pub(crate) fn malformed() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        false,
        "public TikTok response is malformed",
    )
}
/// `UnsupportedUrl` — a content-state outcome (non-zero `statusCode`, missing
/// `itemInfo.itemStruct.video`, slideshow) the host boundary renders as
/// `CaptureOutcome.empty`, never a download failure (spec Req 3 + Req 4;
/// `new-extractor/SKILL.md:159-167`). WIT `unsupported-url` variant of the
/// `resolver-error-kind` enum (`wit/media-url-resolver-v2/wit/
/// media-url-resolver.wit:58-62`).
pub(crate) fn unsupported() -> ResolverError {
    error(
        ResolverErrorKind::UnsupportedUrl,
        false,
        "public TikTok content is unsupported",
    )
}
pub(crate) fn unavailable() -> ResolverError {
    error(
        ResolverErrorKind::Unavailable,
        false,
        "public TikTok media is unavailable",
    )
}
pub(crate) fn private() -> ResolverError {
    error(
        ResolverErrorKind::PrivateOrUnavailable,
        false,
        "public TikTok content is private or unavailable",
    )
}
pub(crate) fn transport(value: HttpsError) -> ResolverError {
    match value {
        HttpsError::Timeout => error(
            ResolverErrorKind::Timeout,
            true,
            "public TikTok retrieval timed out",
        ),
        HttpsError::TransportFailure => error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public TikTok retrieval transport failed",
        ),
        HttpsError::MalformedUpstream
        | HttpsError::ResponseTooLarge
        | HttpsError::InvalidRequest
        | HttpsError::BlockedHost
        | HttpsError::RedirectRejected
        | HttpsError::RequestTooLarge => malformed(),
    }
}
pub(crate) fn status(value: u16) -> Result<(), ResolverError> {
    match value {
        200..=299 => Ok(()),
        401 | 403 => Err(private()),
        404 | 410 => Err(unavailable()),
        429 => Err(error(
            ResolverErrorKind::RateLimited,
            true,
            "public TikTok retrieval was rate limited",
        )),
        500..=599 => Err(error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public TikTok retrieval upstream failed",
        )),
        _ => Err(malformed()),
    }
}

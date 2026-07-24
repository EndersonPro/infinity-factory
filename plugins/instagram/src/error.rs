use bex_media_url_resolver_v2::{HttpsError, ResolverError, ResolverErrorKind};

fn error(kind: ResolverErrorKind, retryable: bool, message: &str) -> ResolverError {
    ResolverError {
        kind,
        retryable,
        safe_message: message.into(),
    }
}
pub(crate) fn invalid_input() -> ResolverError {
    error(
        ResolverErrorKind::InvalidInput,
        false,
        "unsupported public Instagram input",
    )
}
pub(crate) fn policy() -> ResolverError {
    error(
        ResolverErrorKind::PolicyDenied,
        false,
        "public retrieval policy rejected the request",
    )
}
pub(crate) fn malformed() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        false,
        "public retrieval response is malformed",
    )
}
pub(crate) fn private() -> ResolverError {
    error(
        ResolverErrorKind::PrivateOrUnavailable,
        false,
        "public content is private or unavailable",
    )
}
pub(crate) fn unavailable() -> ResolverError {
    error(
        ResolverErrorKind::Unavailable,
        false,
        "public content is unavailable",
    )
}
pub(crate) fn transport(value: HttpsError) -> ResolverError {
    match value {
        HttpsError::Timeout => error(
            ResolverErrorKind::Timeout,
            true,
            "public retrieval timed out",
        ),
        HttpsError::TransportFailure => error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public retrieval transport failed",
        ),
        HttpsError::MalformedUpstream | HttpsError::ResponseTooLarge => malformed(),
        HttpsError::InvalidRequest
        | HttpsError::BlockedHost
        | HttpsError::RedirectRejected
        | HttpsError::RequestTooLarge => policy(),
    }
}
pub(crate) fn status(value: u16) -> Result<(), ResolverError> {
    match value {
        200..=299 => Ok(()),
        401 | 403 => Err(error(
            ResolverErrorKind::PrivateOrUnavailable,
            false,
            "public content is private or unavailable",
        )),
        404 | 410 => Err(error(
            ResolverErrorKind::Unavailable,
            false,
            "public content is unavailable",
        )),
        429 => Err(error(
            ResolverErrorKind::RateLimited,
            true,
            "public retrieval was rate limited",
        )),
        500..=599 => Err(error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public retrieval upstream failed",
        )),
        _ => Err(malformed()),
    }
}

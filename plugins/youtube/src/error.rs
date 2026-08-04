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
        "unsupported public YouTube input",
    )
}
pub(crate) fn malformed() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        false,
        "public YouTube response is malformed",
    )
}
pub(crate) fn private() -> ResolverError {
    error(
        ResolverErrorKind::PrivateOrUnavailable,
        false,
        "public YouTube content is private or unavailable",
    )
}
pub(crate) fn unavailable() -> ResolverError {
    error(
        ResolverErrorKind::Unavailable,
        false,
        "public YouTube content is unavailable",
    )
}
pub(crate) fn transport(value: HttpsError) -> ResolverError {
    match value {
        HttpsError::Timeout => error(
            ResolverErrorKind::Timeout,
            true,
            "public YouTube retrieval timed out",
        ),
        HttpsError::TransportFailure => error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public YouTube retrieval transport failed",
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
            "public YouTube retrieval was rate limited",
        )),
        500..=599 => Err(error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public YouTube retrieval upstream failed",
        )),
        _ => Err(malformed()),
    }
}

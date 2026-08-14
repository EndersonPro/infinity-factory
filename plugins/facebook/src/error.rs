use bex_media_url_resolver_v2::{HttpsError, ResolverError, ResolverErrorKind};

fn error(kind: ResolverErrorKind, retryable: bool, message: &str) -> ResolverError {
    ResolverError {
        kind,
        retryable,
        safe_message: message.into(),
    }
}

/// `InvalidInput` — the source URL is not a canonical Facebook video page
/// (spec Req 1).
pub(crate) fn invalid_input() -> ResolverError {
    error(
        ResolverErrorKind::InvalidInput,
        false,
        "unsupported public Facebook input",
    )
}

/// `MalformedResponse` — the response body is malformed upstream JSON
/// (spec Req 3 / Req 5 malformed Tahoe).
pub(crate) fn malformed() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        false,
        "public Facebook response is malformed",
    )
}

/// `PrivateOrUnavailable` — a login-walled page (spec Req 6).
pub(crate) fn private() -> ResolverError {
    error(
        ResolverErrorKind::PrivateOrUnavailable,
        false,
        "public Facebook content is private or unavailable",
    )
}

pub(crate) fn unavailable() -> ResolverError {
    error(
        ResolverErrorKind::Unavailable,
        false,
        "public Facebook content is unavailable",
    )
}

pub(crate) fn policy() -> ResolverError {
    error(
        ResolverErrorKind::PolicyDenied,
        false,
        "public retrieval policy rejected the request",
    )
}

/// Map an `HttpsError` transport outcome onto a typed `ResolverError`
/// (spec Req 2 / Req 4).
pub(crate) fn transport(value: HttpsError) -> ResolverError {
    match value {
        HttpsError::Timeout => error(
            ResolverErrorKind::Timeout,
            true,
            "public Facebook retrieval timed out",
        ),
        HttpsError::TransportFailure => error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public Facebook retrieval transport failed",
        ),
        HttpsError::MalformedUpstream | HttpsError::ResponseTooLarge => malformed(),
        HttpsError::InvalidRequest
        | HttpsError::BlockedHost
        | HttpsError::RedirectRejected
        | HttpsError::RequestTooLarge => policy(),
    }
}

/// Map an HTTP status onto a typed outcome (spec Req 2).
pub(crate) fn status(value: u16) -> Result<(), ResolverError> {
    match value {
        200..=299 => Ok(()),
        401 | 403 => Err(private()),
        404 | 410 => Err(unavailable()),
        429 => Err(error(
            ResolverErrorKind::RateLimited,
            true,
            "public Facebook retrieval was rate limited",
        )),
        500..=599 => Err(error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public Facebook retrieval upstream failed",
        )),
        _ => Err(malformed()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_errors_map_to_typed_kinds() {
        assert_eq!(invalid_input().kind, ResolverErrorKind::InvalidInput);
        assert_eq!(malformed().kind, ResolverErrorKind::MalformedResponse);
        assert_eq!(private().kind, ResolverErrorKind::PrivateOrUnavailable);
        assert_eq!(unavailable().kind, ResolverErrorKind::Unavailable);
        assert_eq!(policy().kind, ResolverErrorKind::PolicyDenied);
    }

    #[test]
    fn transport_maps_timeout_and_failure_as_retryable() {
        assert_eq!(
            transport(HttpsError::Timeout).kind,
            ResolverErrorKind::Timeout
        );
        assert!(transport(HttpsError::Timeout).retryable);
        assert_eq!(
            transport(HttpsError::TransportFailure).kind,
            ResolverErrorKind::UpstreamFailure
        );
        assert!(transport(HttpsError::TransportFailure).retryable);
    }

    #[test]
    fn transport_maps_malformed_and_policy_failures_closed() {
        assert_eq!(
            transport(HttpsError::MalformedUpstream).kind,
            ResolverErrorKind::MalformedResponse
        );
        assert_eq!(
            transport(HttpsError::ResponseTooLarge).kind,
            ResolverErrorKind::MalformedResponse
        );
        assert_eq!(
            transport(HttpsError::BlockedHost).kind,
            ResolverErrorKind::PolicyDenied
        );
        assert_eq!(
            transport(HttpsError::InvalidRequest).kind,
            ResolverErrorKind::PolicyDenied
        );
        assert_eq!(
            transport(HttpsError::RedirectRejected).kind,
            ResolverErrorKind::PolicyDenied
        );
        assert_eq!(
            transport(HttpsError::RequestTooLarge).kind,
            ResolverErrorKind::PolicyDenied
        );
    }

    #[test]
    fn status_maps_http_outcomes() {
        assert!(status(200).is_ok());
        assert_eq!(
            status(403).unwrap_err().kind,
            ResolverErrorKind::PrivateOrUnavailable
        );
        assert_eq!(
            status(404).unwrap_err().kind,
            ResolverErrorKind::Unavailable
        );
        assert_eq!(status(429).unwrap_err().kind, ResolverErrorKind::RateLimited);
        assert!(status(429).unwrap_err().retryable);
        assert_eq!(
            status(500).unwrap_err().kind,
            ResolverErrorKind::UpstreamFailure
        );
        assert!(status(500).unwrap_err().retryable);
        assert_eq!(
            status(302).unwrap_err().kind,
            ResolverErrorKind::MalformedResponse
        );
    }
}
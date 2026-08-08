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
        "unsupported public X input",
    )
}

pub(crate) fn malformed() -> ResolverError {
    error(
        ResolverErrorKind::MalformedResponse,
        false,
        "public X response is malformed",
    )
}

/// A post that was withdrawn, or that the account has since protected. The
/// endpoint answers a `TweetTombstone` for both, and the difference is not
/// visible from here — nor is it useful to the person who pasted the link.
pub(crate) fn private() -> ResolverError {
    error(
        ResolverErrorKind::PrivateOrUnavailable,
        false,
        "public X post is unavailable",
    )
}

pub(crate) fn unavailable() -> ResolverError {
    error(
        ResolverErrorKind::Unavailable,
        false,
        "public X media is unavailable",
    )
}

pub(crate) fn transport(value: HttpsError) -> ResolverError {
    match value {
        HttpsError::Timeout => error(
            ResolverErrorKind::Timeout,
            true,
            "public X retrieval timed out",
        ),
        HttpsError::TransportFailure => error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public X retrieval transport failed",
        ),
        HttpsError::MalformedUpstream => malformed(),
        HttpsError::BlockedHost | HttpsError::RedirectRejected | HttpsError::InvalidRequest => {
            error(
                ResolverErrorKind::PolicyDenied,
                false,
                "public X retrieval was denied by policy",
            )
        }
        HttpsError::RequestTooLarge | HttpsError::ResponseTooLarge => error(
            ResolverErrorKind::UpstreamFailure,
            false,
            "public X response exceeded transport limits",
        ),
    }
}

/// Map the transport status onto a typed outcome.
///
/// `429` is the one worth keeping apart: the endpoint is undocumented and
/// unmetered as far as anyone outside knows, so a rate limit is the most
/// likely way it says no, and it is the only status here worth retrying.
pub(crate) fn status(code: u16) -> Result<(), ResolverError> {
    match code {
        200 => Ok(()),
        401 | 403 => Err(private()),
        404 | 410 => Err(unavailable()),
        429 => Err(error(
            ResolverErrorKind::RateLimited,
            true,
            "public X retrieval was rate limited",
        )),
        500..=599 => Err(error(
            ResolverErrorKind::UpstreamFailure,
            true,
            "public X upstream failed",
        )),
        _ => Err(malformed()),
    }
}

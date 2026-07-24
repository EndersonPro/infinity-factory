use bex_media_url_resolver::{
    MediaStream, Metadata, Resolution, ResolveRequest, ResolveResponse, ResolverError,
    ResolverErrorKind, ResolverGuest, validate_request, validate_response,
};
use url::Url;

pub struct Component;

impl ResolverGuest for Component {
    fn resolve(request: ResolveRequest) -> Result<ResolveResponse, ResolverError> {
        validate_request(&request)?;
        let url = Url::parse(&request.source_url).map_err(|_| invalid("invalid source URL"))?;
        if url.scheme() != "https"
            || url.username() != ""
            || url.password().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid("source URL must be canonical HTTPS"));
        }
        let supported = url.host_str() == Some("media.example.test")
            && url.port().is_none()
            && url.path() == "/video.mp4"
            && url.query().is_none();
        if !supported {
            return checked(ResolveResponse {
                metadata: None,
                resolution: Resolution::Unsupported(bex_media_url_resolver::Unsupported {
                    reason: "unsupported source".into(),
                }),
            });
        }
        checked(ResolveResponse {
            metadata: Some(Metadata {
                title: Some("Direct URL Fixture".into()),
                author: Some("Infinity Factory".into()),
                thumbnail_url: None,
                duration_milliseconds: None,
            }),
            resolution: Resolution::Direct(MediaStream {
                url: request.source_url,
                format: Some("mp4".into()),
                mime_type: Some("video/mp4".into()),
                quality_label: None,
                codecs: None,
                expires_at_unix_seconds: None,
                byte_range_supported: true,
                headers: vec![],
            }),
        })
    }
}

fn checked(response: ResolveResponse) -> Result<ResolveResponse, ResolverError> {
    validate_response(&response)?;
    Ok(response)
}

fn invalid(message: &str) -> ResolverError {
    ResolverError {
        kind: ResolverErrorKind::InvalidInput,
        retryable: false,
        safe_message: message.into(),
    }
}

#[cfg(target_arch = "wasm32")]
bex_media_url_resolver::export_resolver!(Component);

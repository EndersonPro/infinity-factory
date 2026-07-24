use bex_media_url_resolver::{
    Candidate, MediaStream, Resolution, ResolveIntent, ResolveRequest, ResolveResponse,
    ResolverErrorKind, validate_request, validate_response,
};

fn stream(url: &str) -> MediaStream {
    MediaStream {
        url: url.into(),
        format: None,
        mime_type: None,
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: None,
        byte_range_supported: true,
        headers: vec![],
    }
}

#[test]
fn accepts_bounded_request() {
    let request = ResolveRequest {
        source_url: "https://media.example.test/video.mp4".into(),
        intent: ResolveIntent::Inspect,
        quality_preferences: vec![],
        client_context: vec![],
        correlation_id: "test".into(),
    };
    assert!(validate_request(&request).is_ok());
}

#[test]
fn rejects_oversized_request() {
    let request = ResolveRequest {
        source_url: "x".repeat(2049),
        intent: ResolveIntent::Inspect,
        quality_preferences: vec![],
        client_context: vec![],
        correlation_id: "test".into(),
    };
    assert_eq!(
        validate_request(&request).unwrap_err().kind,
        ResolverErrorKind::InvalidInput
    );
}

#[test]
fn rejects_insecure_streams() {
    let response = ResolveResponse {
        metadata: None,
        resolution: Resolution::Direct(stream("http://unsafe.test/media")),
    };
    assert!(validate_response(&response).is_err());
}

#[test]
fn rejects_duplicate_or_unordered_candidates() {
    for ids in [["b", "a"], ["a", "a"]] {
        let response = ResolveResponse {
            metadata: None,
            resolution: Resolution::Candidates(
                ids.into_iter()
                    .map(|id| Candidate {
                        id: id.into(),
                        stream: stream("https://media.example.test/video.mp4"),
                    })
                    .collect(),
            ),
        };
        assert!(validate_response(&response).is_err());
    }
}

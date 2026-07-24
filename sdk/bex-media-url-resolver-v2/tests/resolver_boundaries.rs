use bex_media_url_resolver_v2::{
    Candidate, ContextEntry, Deferred, MediaStream, Metadata, MuxContainer, MuxPlan,
    QualityPreference, Resolution, ResolveIntent, ResolveRequest, ResolveResponse, ResolverHeader,
    SeparatedStreams, Unsupported, resolver_bounds, validate_resolver_request,
    validate_resolver_response,
};

fn request() -> ResolveRequest {
    ResolveRequest {
        source_url: "https://www.instagram.com/p/ABC_123-/".into(),
        intent: ResolveIntent::Play,
        quality_preferences: vec![],
        client_context: vec![],
        correlation_id: "c".into(),
    }
}
fn quality() -> QualityPreference {
    QualityPreference {
        container: None,
        mime_type: None,
        quality_label: None,
        max_height: None,
        max_bitrate: None,
    }
}
fn stream(url: &str) -> MediaStream {
    MediaStream {
        url: url.into(),
        format: Some("mp4".into()),
        mime_type: Some("video/mp4".into()),
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: None,
        byte_range_supported: true,
        headers: vec![],
    }
}
fn response(resolution: Resolution) -> ResolveResponse {
    ResolveResponse {
        metadata: None,
        resolution,
    }
}
#[test]
fn validates_request_exact_above_and_duplicate_context() {
    let mut value = request();
    value.correlation_id = "x".repeat(resolver_bounds::CORRELATION_ID);
    value.quality_preferences = vec![quality(); resolver_bounds::QUALITY_PREFERENCES];
    value.client_context = (0..resolver_bounds::CONTEXT_ENTRIES)
        .map(|index| ContextEntry {
            key: char::from(b'a' + index as u8).to_string(),
            value: "v".repeat(255),
        })
        .collect();
    assert!(validate_resolver_request(&value).is_ok());
    value.correlation_id.push('x');
    assert!(validate_resolver_request(&value).is_err());
    value = request();
    value.client_context = vec![
        ContextEntry {
            key: "locale".into(),
            value: "en".into(),
        },
        ContextEntry {
            key: "locale".into(),
            value: "es".into(),
        },
    ];
    assert!(validate_resolver_request(&value).is_err());
    value = request();
    value.quality_preferences = vec![quality(); resolver_bounds::QUALITY_PREFERENCES + 1];
    assert!(validate_resolver_request(&value).is_err());
    value = request();
    value.client_context = vec![ContextEntry {
        key: "locale".into(),
        value: "en\nsecret".into(),
    }];
    assert!(validate_resolver_request(&value).is_err());
}
#[test]
fn preserves_carousel_order_and_rejects_ambiguous_candidates() {
    let items = vec![
        Candidate {
            id: "slide-z".into(),
            stream: stream("https://cdn.example/z.mp4"),
        },
        Candidate {
            id: "slide-a".into(),
            stream: stream("https://cdn.example/a.mp4"),
        },
    ];
    let mut value = response(Resolution::Candidates(items));
    validate_resolver_response(&value).unwrap();
    let Resolution::Candidates(items) = &value.resolution else {
        unreachable!()
    };
    assert_eq!(
        items
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        ["slide-z", "slide-a"]
    );
    let Resolution::Candidates(items) = &mut value.resolution else {
        unreachable!()
    };
    items[1].id = items[0].id.clone();
    assert!(validate_resolver_response(&value).is_err());
    let items = (0..=resolver_bounds::CANDIDATES)
        .map(|index| Candidate {
            id: index.to_string(),
            stream: stream(&format!("https://cdn.example/{index}.mp4")),
        })
        .collect();
    assert!(validate_resolver_response(&response(Resolution::Candidates(items))).is_err());
}
#[test]
fn validates_stream_urls_headers_and_separated_media() {
    let mut exact = stream("https://cdn.example/a.mp4");
    exact.headers = (0..8)
        .map(|index| ResolverHeader {
            name: char::from(b'a' + index).to_string(),
            value: "v".repeat(1023),
        })
        .collect();
    assert!(validate_resolver_response(&response(Resolution::Direct(exact))).is_ok());
    let mut invalid = stream("http://cdn.example/a.mp4");
    assert!(validate_resolver_response(&response(Resolution::Direct(invalid))).is_err());
    invalid = stream("https://cdn.example/a.mp4");
    invalid.headers = vec![
        ResolverHeader {
            name: "accept".into(),
            value: "a".into(),
        },
        ResolverHeader {
            name: "Accept".into(),
            value: "b".into(),
        },
    ];
    assert!(validate_resolver_response(&response(Resolution::Direct(invalid))).is_err());
    invalid = stream("https://cdn.example/a.mp4");
    invalid.headers = vec![ResolverHeader {
        name: "x-forwarded-host".into(),
        value: "evil.example".into(),
    }];
    assert!(validate_resolver_response(&response(Resolution::Direct(invalid))).is_err());
    let separated = SeparatedStreams {
        audio: stream("https://cdn.example/a.m4a"),
        video: stream("https://cdn.example/v.mp4"),
        mux_plan: MuxPlan {
            container: MuxContainer::Mp4,
            prefer_stream_copy: true,
        },
    };
    assert!(validate_resolver_response(&response(Resolution::Separated(separated))).is_ok());
}
#[test]
fn validates_metadata_reason_and_retry_boundaries() {
    let mut value = response(Resolution::Unsupported(Unsupported {
        reason: "r".repeat(resolver_bounds::REASON),
    }));
    value.metadata = Some(Metadata {
        title: Some("t".repeat(resolver_bounds::TITLE)),
        author: Some("a".repeat(resolver_bounds::AUTHOR)),
        thumbnail_url: Some("https://cdn.example/thumb.jpg".into()),
        duration_milliseconds: Some(1),
    });
    assert!(validate_resolver_response(&value).is_ok());
    value.metadata.as_mut().unwrap().title = Some("unsafe\nvalue".into());
    assert!(validate_resolver_response(&value).is_err());
    value.metadata.as_mut().unwrap().title = None;
    let Resolution::Unsupported(item) = &mut value.resolution else {
        unreachable!()
    };
    item.reason.push('r');
    assert!(validate_resolver_response(&value).is_err());
    value = response(Resolution::Deferred(Deferred {
        retry_after_seconds: Some(resolver_bounds::RETRY_AFTER),
        reason: "later".into(),
    }));
    assert!(validate_resolver_response(&value).is_ok());
    let Resolution::Deferred(item) = &mut value.resolution else {
        unreachable!()
    };
    item.retry_after_seconds = Some(resolver_bounds::RETRY_AFTER + 1);
    assert!(validate_resolver_response(&value).is_err());
}

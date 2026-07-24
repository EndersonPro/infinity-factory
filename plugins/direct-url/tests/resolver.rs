use bex_media_url_resolver::{Resolution, ResolveIntent, ResolveRequest, ResolverGuest};
use direct_url::Component;

fn request(url: &str) -> ResolveRequest {
    ResolveRequest {
        source_url: url.into(),
        intent: ResolveIntent::Inspect,
        quality_preferences: vec![],
        client_context: vec![],
        correlation_id: "test".into(),
    }
}

#[test]
fn resolves_exact_https_fixture() {
    let result = Component::resolve(request("https://media.example.test/video.mp4")).unwrap();
    assert!(matches!(result.resolution, Resolution::Direct(_)));
}

#[test]
fn returns_unsupported_for_other_https_urls() {
    let result = Component::resolve(request("https://media.example.test/other")).unwrap();
    assert!(matches!(result.resolution, Resolution::Unsupported(_)));
}

#[test]
fn rejects_unsafe_or_ambiguous_urls() {
    for url in [
        "http://media.example.test/video.mp4",
        "https://user@media.example.test/video.mp4",
        "https://media.example.test/video.mp4#fragment",
    ] {
        assert!(Component::resolve(request(url)).is_err());
    }
    let lookalike =
        Component::resolve(request("https://media.example.test.evil.test/video.mp4")).unwrap();
    assert!(matches!(lookalike.resolution, Resolution::Unsupported(_)));
}

use bandcamp::resolve_public_at;
use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, HttpsError, HttpsResponse, MockHttpsClient, Resolution,
    ResolverErrorKind,
};

const URL: &str = "https://relapsealumni.bandcamp.com/track/hail-to-fire";
const DIRECT_JSON: &str = r#"{"current":{"id":100,"title":"Song","artist":"Artist"},"trackinfo":[{"track_id":100,"title":"Song","duration":9.5,"file":{"mp3-128":"https://t4.bcbits.com/stream/abc?ts=2000000&token=2000000xyz"}}]}"#;
const OG: &str = "https://f4.bcbits.com/img/a1_5.jpg";

fn encode(json: &str) -> String {
    json.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
        .replace('"', "&quot;")
}

fn body(json: &str, og: Option<&str>) -> Vec<u8> {
    let og_tag = match og {
        Some(url) => format!(r#"<meta property="og:image" content="{url}">"#),
        None => String::new(),
    };
    format!(
        "<html><head>{og_tag}</head><body><div data-tralbum=\"{}\"></div></body></html>",
        encode(json)
    )
    .into_bytes()
}

fn resp(final_url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: final_url.into(),
        headers: vec![],
        body: body.to_vec(),
    }
}

fn get(result: Result<HttpsResponse, HttpsError>) -> ExpectedCall {
    ExpectedCall::Get(
        GetRequest {
            url: URL.into(),
            headers: vec![],
        },
        result,
    )
}

#[test]
fn resolves_one_direct_format_with_exactly_one_headerless_get() {
    let mut client =
        MockHttpsClient::new(vec![get(Ok(resp(URL, 200, &body(DIRECT_JSON, Some(OG)))))]);
    let response = resolve_public_at(&mut client, URL, 500_000).unwrap();
    assert_eq!(
        response.metadata.unwrap().thumbnail_url.as_deref(),
        Some(OG)
    );
    assert!(matches!(response.resolution, Resolution::Direct(_)));
    assert_eq!(
        client
            .observations()
            .iter()
            .map(|o| o.operation)
            .collect::<Vec<_>>(),
        ["get"]
    );
    assert!(client.verify().is_ok());
}

#[test]
fn deterministic_success_replays_identically() {
    let mut client = MockHttpsClient::new(vec![
        get(Ok(resp(URL, 200, &body(DIRECT_JSON, None)))),
        get(Ok(resp(URL, 200, &body(DIRECT_JSON, None)))),
    ]);
    let first = resolve_public_at(&mut client, URL, 0).unwrap();
    let second = resolve_public_at(&mut client, URL, 0).unwrap();
    assert_eq!(
        first.metadata.unwrap().title,
        second.metadata.unwrap().title
    );
    assert_eq!(client.observations().len(), 2);
    assert!(client.verify().is_ok());
}

#[test]
fn rejects_invalid_source_with_zero_host_calls() {
    let mut client = MockHttpsClient::new(vec![get(Err(HttpsError::TransportFailure))]);
    let error = resolve_public_at(&mut client, "https://evil.example/track/x", 0).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::InvalidInput);
    assert!(client.observations().is_empty());
    assert!(client.verify().is_err());
}

#[test]
fn maps_transport_and_status_failures_without_retry_forgetfulness_or_leaks() {
    for (result, kind, retryable) in [
        (Err(HttpsError::Timeout), ResolverErrorKind::Timeout, true),
        (
            Err(HttpsError::TransportFailure),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
        (
            Ok(resp(URL, 401, b"SENSITIVE_BODY")),
            ResolverErrorKind::PrivateOrUnavailable,
            false,
        ),
        (
            Ok(resp(URL, 403, b"SENSITIVE_BODY")),
            ResolverErrorKind::PrivateOrUnavailable,
            false,
        ),
        (
            Ok(resp(URL, 404, b"SENSITIVE_BODY")),
            ResolverErrorKind::Unavailable,
            false,
        ),
        (
            Ok(resp(URL, 410, b"SENSITIVE_BODY")),
            ResolverErrorKind::Unavailable,
            false,
        ),
        (
            Ok(resp(URL, 429, b"SENSITIVE_BODY")),
            ResolverErrorKind::RateLimited,
            true,
        ),
        (
            Ok(resp(URL, 500, b"SENSITIVE_BODY")),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
        (
            Ok(resp(URL, 503, b"SENSITIVE_BODY")),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
        (
            Ok(resp(URL, 418, b"SENSITIVE_BODY")),
            ResolverErrorKind::MalformedResponse,
            false,
        ),
        (
            Ok(resp(URL, 200, b"<html></html>")),
            ResolverErrorKind::MalformedResponse,
            false,
        ),
    ] {
        let mut client = MockHttpsClient::new(vec![get(result)]);
        let error = resolve_public_at(&mut client, URL, 0).unwrap_err();
        assert_eq!((error.kind, error.retryable), (kind, retryable));
        assert_eq!(client.observations().len(), 1);
        assert!(client.verify().is_ok());
        assert!(!format!("{error:?}").contains("SENSITIVE_BODY"));
    }
}

#[test]
fn rejects_off_origin_final_response_as_malformed() {
    let mut client = MockHttpsClient::new(vec![get(Ok(resp(
        "https://evil.example/track/x",
        200,
        &body(DIRECT_JSON, None),
    )))]);
    assert_eq!(
        resolve_public_at(&mut client, URL, 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
    assert_eq!(client.observations().len(), 1);
}

#[test]
fn drops_expired_audio_to_unavailable_but_keeps_unexpired() {
    let expired = body(DIRECT_JSON, None);
    let mut client = MockHttpsClient::new(vec![get(Ok(resp(URL, 200, &expired)))]);
    assert_eq!(
        resolve_public_at(&mut client, URL, 3_000_000)
            .unwrap_err()
            .kind,
        ResolverErrorKind::Unavailable
    );
    let mut client = MockHttpsClient::new(vec![get(Ok(resp(URL, 200, &expired)))]);
    let response = resolve_public_at(&mut client, URL, 1_500_000).unwrap();
    assert!(matches!(response.resolution, Resolution::Direct(_)));
}

#[test]
fn resolves_candidates_when_multiple_formats_present() {
    let json = r#"{"current":{"id":1,"title":"T","artist":"A"},"trackinfo":[{"track_id":1,"title":"T","duration":1,"file":{"mp3-128":"https://t4.bcbits.com/a?ts=2000000&token=2000000a","mp3-320":"https://t4.bcbits.com/b?ts=2000000&token=2000000b"}}]}"#;
    let mut client = MockHttpsClient::new(vec![get(Ok(resp(URL, 200, &body(json, None))))]);
    let response = resolve_public_at(&mut client, URL, 0).unwrap();
    assert!(matches!(response.resolution, Resolution::Candidates(items) if items.len() == 2));
    assert_eq!(client.observations().len(), 1);
}

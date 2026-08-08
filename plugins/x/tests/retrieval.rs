use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, Header, HttpsError, HttpsResponse, MockHttpsClient, Resolution,
    ResolverErrorKind, validate_get_request,
};
use x::{request_url, resolve_public};

const SINGLE: &[u8] = include_bytes!("fixtures/video_single_ish.json");
const SOURCE: &str = "https://x.com/LisPower1/status/1001551623938805763";
const EXPECTED_URL: &str =
    "https://cdn.syndication.twimg.com/tweet-result?id=1001551623938805763&token=2fegtisuq5";

fn expected_get() -> GetRequest {
    GetRequest {
        url: EXPECTED_URL.into(),
        headers: vec![],
    }
}

fn ok(body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: EXPECTED_URL.into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "application/json".into(),
        }],
        body: body.to_vec(),
    }
}

fn failing(status: u16) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: EXPECTED_URL.into(),
        headers: vec![],
        body: Vec::new(),
    }
}

#[test]
fn builds_the_request_from_the_post_id_alone() {
    // Whatever the share sheet appended is gone by the time a URL is built.
    for source in [
        SOURCE,
        "https://twitter.com/someoneelse/status/1001551623938805763?t=x&s=20",
        "https://x.com/a/status/1001551623938805763/photo/1",
    ] {
        let id = x::classify_url(source).expect("accepted");
        assert_eq!(request_url(&id), EXPECTED_URL, "{source}");
    }
}

/// The URL this plugin emits has to survive the transport gate it will be
/// handed to. Asserting it here means a drift in either one fails a test
/// rather than a device.
#[test]
fn emits_a_url_the_transport_admits() {
    assert!(validate_get_request(&expected_get()).is_ok());
}

#[test]
fn issues_exactly_one_headerless_get() {
    let mut client = MockHttpsClient::new(vec![ExpectedCall::Get(expected_get(), Ok(ok(SINGLE)))]);

    let response = resolve_public(&mut client, SOURCE).expect("resolves");

    client.verify().expect("one GET, exactly as expected");
    assert_eq!(client.observations().len(), 1);
    assert!(matches!(
        response.resolution,
        Resolution::Candidates(_) | Resolution::Direct(_)
    ));
}

#[test]
fn rejects_an_unsupported_source_before_touching_the_host() {
    let mut client = MockHttpsClient::new(vec![]);

    let error = resolve_public(&mut client, "https://x.com/a/spaces/1").expect_err("refused");

    assert_eq!(error.kind, ResolverErrorKind::InvalidInput);
    assert!(client.observations().is_empty(), "no host call may happen");
}

#[test]
fn maps_transport_status_onto_typed_outcomes() {
    for (status, kind, retryable) in [
        (403u16, ResolverErrorKind::PrivateOrUnavailable, false),
        (404, ResolverErrorKind::Unavailable, false),
        (429, ResolverErrorKind::RateLimited, true),
        (503, ResolverErrorKind::UpstreamFailure, true),
    ] {
        let mut client =
            MockHttpsClient::new(vec![ExpectedCall::Get(expected_get(), Ok(failing(status)))]);
        let error = resolve_public(&mut client, SOURCE).expect_err("failed");
        assert_eq!(error.kind, kind, "status {status}");
        assert_eq!(error.retryable, retryable, "status {status}");
    }
}

#[test]
fn maps_transport_errors_onto_typed_outcomes() {
    for (transport, kind) in [
        (HttpsError::Timeout, ResolverErrorKind::Timeout),
        (
            HttpsError::TransportFailure,
            ResolverErrorKind::UpstreamFailure,
        ),
        (HttpsError::BlockedHost, ResolverErrorKind::PolicyDenied),
        (
            HttpsError::MalformedUpstream,
            ResolverErrorKind::MalformedResponse,
        ),
    ] {
        let mut client =
            MockHttpsClient::new(vec![ExpectedCall::Get(expected_get(), Err(transport))]);
        let error = resolve_public(&mut client, SOURCE).expect_err("failed");
        assert_eq!(error.kind, kind, "{transport:?}");
    }
}

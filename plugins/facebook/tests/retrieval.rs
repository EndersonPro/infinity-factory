use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, Header, HttpsError, HttpsResponse, MockHttpsClient, Resolution,
    ResolverErrorKind, TahoeExpectation,
};
use facebook::resolve_public;

const ACCEPT: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";
const URL: &str = "https://www.facebook.com/zuck/videos/10107927396957931/";
const TAHOE_URL: &str = "https://www.facebook.com/video/tahoe/async/10107927396957931/";

const FB_VIDEO: &str = include_str!("fixtures/fb_video.html");
const FB_TAHOE_PAGE: &str = include_str!("fixtures/fb_tahoe_page.html");
const FB_LOGIN_WALL: &str = include_str!("fixtures/fb_login_wall.html");
const FB_NO_TOKENS: &str = include_str!("fixtures/fb_no_tokens.html");
const FB_TAHOE_JSON: &[u8] = include_bytes!("fixtures/fb_tahoe.json");

fn https(url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: url.into(),
        headers: vec![],
        body: body.into(),
    }
}

fn get_headers() -> Vec<Header> {
    vec![
        Header {
            name: "accept".into(),
            value: ACCEPT.into(),
        },
        Header {
            name: "accept-language".into(),
            value: ACCEPT_LANGUAGE.into(),
        },
    ]
}

fn get(body: &[u8]) -> ExpectedCall {
    ExpectedCall::Get(
        GetRequest {
            url: URL.into(),
            headers: get_headers(),
        },
        Ok(https(URL, 200, body)),
    )
}

fn get_err(err: HttpsError) -> ExpectedCall {
    ExpectedCall::Get(
        GetRequest {
            url: URL.into(),
            headers: get_headers(),
        },
        Err(err),
    )
}

fn tahoe(body: &[u8]) -> ExpectedCall {
    ExpectedCall::Tahoe(TahoeExpectation::new(
        TAHOE_URL,
        17,
        "PHASED:DEFAULT",
        "100123456",
        Ok(https(TAHOE_URL, 200, body)),
    ))
}

fn operations(client: &MockHttpsClient) -> Vec<&'static str> {
    client
        .observations()
        .iter()
        .map(|item| item.operation)
        .collect()
}

#[test]
fn get_then_progressive_urls_resolve_as_candidates_with_metadata() {
    let mut client = MockHttpsClient::new(vec![get(FB_VIDEO.as_bytes())]);
    let response = resolve_public(&mut client, URL).expect("happy path resolves");
    let Resolution::Candidates(items) = response.resolution else {
        panic!("expected candidates for distinct SD+HD progressive URLs");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].stream.url, "https://video.example.invalid/sd.mp4");
    assert_eq!(items[1].stream.url, "https://video.example.invalid/hd.mp4");
    let metadata = response.metadata.expect("metadata emitted for a valid video");
    assert_eq!(metadata.title.as_deref(), Some("Public video"));
    assert_eq!(metadata.author.as_deref(), Some("Zuck"));
    assert_eq!(
        metadata.thumbnail_url.as_deref(),
        Some("https://scontent.example.invalid/thumb.jpg")
    );
    assert_eq!(metadata.duration_milliseconds, Some(90_000));
    assert_eq!(operations(&client), ["get"]);
    assert!(client.verify().is_ok());
}

#[test]
fn get_without_progressive_falls_back_to_tahoe_sd_hd_candidates() {
    let mut client = MockHttpsClient::new(vec![get(FB_TAHOE_PAGE.as_bytes()), tahoe(FB_TAHOE_JSON)]);
    let response = resolve_public(&mut client, URL).expect("tahoe fallback resolves");
    let Resolution::Candidates(items) = response.resolution else {
        panic!("expected candidates from tahoe sd_src/hd_src");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(
        items[0].stream.url,
        "https://video.example.invalid/tahoe-sd.mp4"
    );
    assert_eq!(
        items[1].stream.url,
        "https://video.example.invalid/tahoe-hd.mp4"
    );
    assert_eq!(operations(&client), ["get", "post-tahoe"]);
    assert!(client.verify().is_ok());
    // The ephemeral fb_dtsg never leaks into observed call records.
    assert!(
        !format!("{:?}", client.observations()).contains("SENSITIVE_FB_DTSG"),
        "fb_dtsg secret leaked"
    );
}

#[test]
fn login_wall_maps_to_private_or_unavailable_without_tahoe() {
    let mut client = MockHttpsClient::new(vec![get(FB_LOGIN_WALL.as_bytes())]);
    let error = resolve_public(&mut client, URL).expect_err("login wall is an error");
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert_eq!(operations(&client), ["get"]);
    assert!(client.verify().is_ok());
}

#[test]
fn get_transport_failure_maps_to_upstream_failure() {
    let mut client = MockHttpsClient::new(vec![get_err(HttpsError::TransportFailure)]);
    let error = resolve_public(&mut client, URL).expect_err("transport failure is an error");
    assert_eq!(error.kind, ResolverErrorKind::UpstreamFailure);
    assert_eq!(operations(&client), ["get"]);
    assert!(client.verify().is_ok());
}

#[test]
fn tahoe_tokens_absent_maps_to_unsupported_without_tahoe_call() {
    let mut client = MockHttpsClient::new(vec![get(FB_NO_TOKENS.as_bytes())]);
    let response = resolve_public(&mut client, URL).expect("unsupported is a response");
    assert!(matches!(response.resolution, Resolution::Unsupported(_)));
    assert!(response.metadata.is_none());
    assert_eq!(operations(&client), ["get"]);
    assert!(client.verify().is_ok());
}
use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, HttpsError, HttpsResponse, MockHttpsClient, PublicGraphqlExpectation,
    Resolution, ResolverErrorKind,
};
use instagram::resolve_public;

const URL: &str = "https://www.instagram.com/p/zzzzzzzzzzzzzzzzzzzzzz/";
const VARIABLES: &str = r#"{"media_id":"4407466847737869622001804439116235881715"}"#;
const BASIC: &str = include_str!("../fixtures/page-basic.html");
const RELAY: &str = include_str!("../fixtures/page-relay-valid.html");
const DIRECT: &[u8] = include_bytes!("../fixtures/graphql-direct.json");
const CAROUSEL: &[u8] = include_bytes!("../fixtures/graphql-carousel.json");
const IMAGE: &[u8] = include_bytes!("../fixtures/graphql-image.json");
const DRIFT: &[u8] = include_bytes!("../fixtures/graphql-drift.json");

fn response(url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: url.into(),
        headers: vec![],
        body: body.into(),
    }
}
fn plan(page: &str, graphql: Result<HttpsResponse, HttpsError>) -> MockHttpsClient {
    let page = page.replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    MockHttpsClient::new(vec![
        ExpectedCall::Get(
            GetRequest {
                url: URL.into(),
                headers: vec![],
            },
            Ok(response(URL, 200, page.as_bytes())),
        ),
        ExpectedCall::Graphql(PublicGraphqlExpectation::new(
            "https://www.instagram.com/api/graphql",
            "PolarisLoggedOutDesktopWWWPostRootContentQuery",
            "27130156389949648",
            VARIABLES,
            18,
            graphql,
        )),
    ])
}
fn graphql(body: &[u8]) -> Result<HttpsResponse, HttpsError> {
    Ok(response("https://www.instagram.com/api/graphql", 200, body))
}

#[test]
fn maps_direct_video_with_safe_metadata() {
    let mut client = plan(BASIC, graphql(DIRECT));
    let result = resolve_public(&mut client, URL).unwrap();
    assert_eq!(
        result.metadata.as_ref().unwrap().title.as_deref(),
        Some("Direct video")
    );
    let Resolution::Direct(stream) = result.resolution else {
        panic!("expected direct")
    };
    assert_eq!(stream.url, "https://cdn.example.invalid/direct.mp4");
    assert_eq!(stream.mime_type.as_deref(), Some("video/mp4"));
    assert!(client.verify().is_ok());
}

#[test]
fn preserves_complete_mixed_carousel_order() {
    for _ in 0..2 {
        let mut client = plan(BASIC, graphql(CAROUSEL));
        let result = resolve_public(&mut client, URL).unwrap();
        let Resolution::Candidates(items) = result.resolution else {
            panic!("expected candidates")
        };
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["media-0000", "media-0001"]
        );
        assert_eq!(
            items
                .iter()
                .map(|item| item.stream.url.as_str())
                .collect::<Vec<_>>(),
            [
                "https://cdn.example.invalid/first.jpg",
                "https://cdn.example.invalid/second.mp4",
            ]
        );
        assert!(client.verify().is_ok());
    }
}

#[test]
fn returns_unsupported_for_single_image() {
    let mut client = plan(BASIC, graphql(IMAGE));
    let result = resolve_public(&mut client, URL).unwrap();
    assert!(matches!(result.resolution, Resolution::Unsupported(_)));
    assert!(client.verify().is_ok());
}

#[test]
fn uses_same_page_relay_only_for_graphql_schema_drift() {
    let mut client = plan(RELAY, graphql(DRIFT));
    let result = resolve_public(&mut client, URL).unwrap();
    let Resolution::Direct(stream) = result.resolution else {
        panic!("expected relay direct")
    };
    assert_eq!(stream.url, "https://cdn.example.invalid/relay.mp4");
    assert_eq!(client.observations().len(), 2);
    assert!(client.verify().is_ok());
    assert!(!format!("{:?}", client.observations()).contains("SENSITIVE_SENTINEL"));
}

#[test]
fn accepts_query_bearing_share_url_and_strips_query_before_get() {
    // A real Instagram share link carries allowlisted query keys
    // (`igsh`/`utm_source`/`utm_medium`). The exported Wasm Guest routes these
    // straight through `resolve_public` without `validate_resolver_request`,
    // relying on `classify_url` to accept and DISCARD the query before any
    // network call. Only the canonical, query-free URL may reach the host GET.
    const SHARE: &str = "https://www.instagram.com/reel/zzzzzzzzzzzzzzzzzzzzzz/?igsh=SYNTHETIC123&utm_source=ig_web_copy_link&utm_medium=share_sheet";
    const CANONICAL: &str = "https://www.instagram.com/reel/zzzzzzzzzzzzzzzzzzzzzz/";
    assert!(SHARE.contains("?igsh="), "input must carry a query string");
    assert!(
        !CANONICAL.contains('?'),
        "GET expectation must be query-free"
    );

    let page = BASIC.replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    let mut client = MockHttpsClient::new(vec![
        // Strict URL equality: if the query were not stripped, the request URL
        // would still carry `igsh`/`utm_*` and fail to match this expectation.
        ExpectedCall::Get(
            GetRequest {
                url: CANONICAL.into(),
                headers: vec![],
            },
            Ok(response(CANONICAL, 200, page.as_bytes())),
        ),
        ExpectedCall::Graphql(PublicGraphqlExpectation::new(
            "https://www.instagram.com/api/graphql",
            "PolarisLoggedOutDesktopWWWPostRootContentQuery",
            "27130156389949648",
            VARIABLES,
            18,
            graphql(DIRECT),
        )),
    ]);

    let result = resolve_public(&mut client, SHARE).unwrap();
    let Resolution::Direct(stream) = result.resolution else {
        panic!("expected direct video from query-bearing share link")
    };
    assert_eq!(stream.url, "https://cdn.example.invalid/direct.mp4");
    assert_eq!(
        client
            .observations()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>(),
        ["get", "post-public-graphql"]
    );
    // Success plus a clean verify proves the GET matched the canonical,
    // query-free URL exactly: no `igsh`/`utm_*` value leaked to the network.
    assert!(client.verify().is_ok());
    let rendered = format!("{:?}", client.observations());
    assert!(!rendered.contains("SYNTHETIC123"));
    assert!(!rendered.contains("igsh"));
    assert!(!rendered.contains("SENSITIVE_SENTINEL"));
}

#[test]
fn rejects_partial_duplicate_or_forbidden_fallback() {
    let duplicate = String::from_utf8(CAROUSEL.to_vec()).unwrap().replace(
        "https://cdn.example.invalid/second.mp4",
        "https://cdn.example.invalid/first.jpg",
    );
    for body in [duplicate.as_bytes(), br#"{"data":{"xig_polaris_media":{"if_not_gated_logged_out":{"is_video":false,"display_url":"https://cdn.example.invalid/root.jpg","edge_sidecar_to_children":{"edges":[{"unknown":{}}]}}}}}"#] {
        let mut client = plan(BASIC, graphql(body));
        let error = resolve_public(&mut client, URL).unwrap_err();
        assert_eq!(error.kind, ResolverErrorKind::MalformedResponse);
    }
    let mut client = plan(
        RELAY,
        Ok(response(
            "https://www.instagram.com/api/graphql",
            403,
            b"SENSITIVE_BODY",
        )),
    );
    let error = resolve_public(&mut client, URL).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert_eq!(client.observations().len(), 2);
    let mut client = plan(
        RELAY,
        graphql(br#"{"data":{"xig_polaris_media":{"is_private":true}}}"#),
    );
    let error = resolve_public(&mut client, URL).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert_eq!(client.observations().len(), 2);
}

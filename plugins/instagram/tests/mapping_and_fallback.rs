use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, HttpsError, HttpsResponse, MockHttpsClient, PublicGraphqlExpectation,
    Resolution, ResolverErrorKind, validate_resolver_response,
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

const CURRENT_VIDEO: &[u8] = include_bytes!("../fixtures/graphql-current-video.json");
const CURRENT_IMAGE: &[u8] = include_bytes!("../fixtures/graphql-current-image.json");
const CURRENT_CAROUSEL: &[u8] = include_bytes!("../fixtures/graphql-current-carousel.json");
const AGE_GATED: &[u8] = include_bytes!("../fixtures/graphql-age-gated.json");
const CDN: &str = "https://instagram.fxxx.fna.fbcdn.net/";

#[test]
fn maps_current_video_versions_with_null_caption() {
    let mut client = plan(BASIC, graphql(CURRENT_VIDEO));
    let result = resolve_public(&mut client, URL).unwrap();
    let metadata = result.metadata.as_ref().unwrap();
    assert_eq!(metadata.title, None);
    assert_eq!(metadata.author.as_deref(), Some("public_user"));
    assert_eq!(
        metadata.thumbnail_url.as_deref(),
        Some(format!("{CDN}v/t51/video-thumb.jpg").as_str())
    );
    let Resolution::Direct(stream) = result.resolution else {
        panic!("expected direct")
    };
    assert_eq!(stream.url, format!("{CDN}o1/v/t2/video-best.mp4"));
    assert_eq!(stream.mime_type.as_deref(), Some("video/mp4"));
    assert!(client.verify().is_ok());
}

#[test]
fn maps_current_carousel_media_in_order() {
    let mut client = plan(BASIC, graphql(CURRENT_CAROUSEL));
    let result = resolve_public(&mut client, URL).unwrap();
    assert_eq!(
        result.metadata.as_ref().unwrap().title.as_deref(),
        Some("Current carousel")
    );
    let Resolution::Candidates(items) = result.resolution else {
        panic!("expected candidates")
    };
    assert_eq!(
        items
            .iter()
            .map(|item| item.stream.url.as_str())
            .collect::<Vec<_>>(),
        [
            format!("{CDN}v/t51/first.jpg"),
            format!("{CDN}o1/v/t2/second.mp4")
        ]
    );
    assert!(client.verify().is_ok());
}

#[test]
fn returns_unsupported_for_current_single_image() {
    let mut client = plan(BASIC, graphql(CURRENT_IMAGE));
    let result = resolve_public(&mut client, URL).unwrap();
    assert!(matches!(result.resolution, Resolution::Unsupported(_)));
    assert_eq!(
        result.metadata.unwrap().thumbnail_url.as_deref(),
        Some(format!("{CDN}v/t51/image.jpg").as_str())
    );
}

#[test]
fn maps_gating_ruling_to_private_without_relay_fallback() {
    let mut client = plan(RELAY, graphql(AGE_GATED));
    let error = resolve_public(&mut client, URL).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert!(!error.retryable);
    assert_eq!(
        error.safe_message,
        "Instagram requires a signed-in account for this post"
    );
    assert!(client.verify().is_ok());
}

#[test]
fn normalizes_long_multiline_real_world_captions_into_a_valid_title() {
    // Live captions are multi-line and routinely exceed the 256-byte SDK
    // title bound; both would make the guest's response validation reject
    // the post. Whitespace runs collapse to one space and the result
    // shortens on a char boundary.
    let caption = r"Tres Métodos de Finanzas en pareja\n\n\tCuando tomas la decisión ".repeat(8);
    let body = String::from_utf8(CURRENT_CAROUSEL.to_vec())
        .unwrap()
        .replace("Current carousel", &caption);
    let mut client = plan(BASIC, graphql(body.as_bytes()));
    let result = resolve_public(&mut client, URL).unwrap();
    assert!(validate_resolver_response(&result).is_ok());
    let title = result.metadata.unwrap().title.unwrap();
    assert!(title.len() <= 256 && title.len() > 250);
    assert!(title.starts_with("Tres Métodos de Finanzas en pareja Cuando tomas la decisión Tres"));
}

#[test]
fn triangulates_current_shape_gating_and_missing_video() {
    let video = String::from_utf8(CURRENT_VIDEO.to_vec()).unwrap();
    let ruling = r#""gating_ruling":{"gating_type":3}"#;
    // A gating ruling beside usable media does not block it.
    let open = video.replace(r#""gating_ruling":null"#, ruling);
    // A video without any usable `video_versions` is malformed, but becomes
    // the signed-in error once a gating ruling explains the missing media.
    let missing = video.replace("video_versions", "absent_versions");
    let gated_missing = missing.replace(r#""gating_ruling":null"#, ruling);
    let cases = [
        (open.as_str(), None),
        (missing.as_str(), Some(ResolverErrorKind::MalformedResponse)),
        (
            gated_missing.as_str(),
            Some(ResolverErrorKind::PrivateOrUnavailable),
        ),
    ];
    for (body, expected) in cases {
        let mut client = plan(BASIC, graphql(body.as_bytes()));
        let result = resolve_public(&mut client, URL);
        assert_eq!(result.err().map(|error| error.kind), expected);
    }
}

use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, HttpsError, HttpsResponse, MockHttpsClient, PublicGraphqlExpectation,
    ResolverErrorKind,
};
use instagram::{retrieve_public, validate_retrieval_config};

const CONFIG: &str = include_str!("../fixtures/retrieval-config.json");
const PAGE: &str = include_str!("../fixtures/page-basic.html");
const GRAPHQL: &[u8] = include_bytes!("../fixtures/graphql-success.json");
const URL: &str = "https://www.instagram.com/p/zzzzzzzzzzzzzzzzzzzzzz/";
const SOURCE: &str = "https://instagram.com/p/zzzzzzzzzzzzzzzzzzzzzz/?utm_source=copy_link";
const VARIABLES: &str = r#"{"media_id":"4407466847737869622001804439116235881715"}"#;

fn response(url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: url.into(),
        headers: vec![],
        body: body.into(),
    }
}
fn page() -> Vec<u8> {
    PAGE.replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL")
        .into_bytes()
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
fn post(result: Result<HttpsResponse, HttpsError>) -> ExpectedCall {
    ExpectedCall::Graphql(PublicGraphqlExpectation::new(
        "https://www.instagram.com/api/graphql",
        "PolarisLoggedOutDesktopWWWPostRootContentQuery",
        "27130156389949648",
        VARIABLES,
        18,
        result,
    ))
}
#[test]
fn retrieves_with_exact_get_then_configured_post() {
    let mut client = MockHttpsClient::new(vec![
        get(Ok(response(URL, 200, &page()))),
        post(Ok(response(
            "https://www.instagram.com/api/graphql",
            200,
            GRAPHQL,
        ))),
    ]);
    let result = retrieve_public(&mut client, SOURCE).unwrap();
    assert_eq!(result.graphql_body(), GRAPHQL);
    assert!(result.relay_json().is_none());
    assert_eq!(
        client
            .observations()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>(),
        ["get", "post-public-graphql"]
    );
    assert!(client.verify().is_ok());
    let rendered = format!("{:?}", client.observations());
    assert!(!rendered.contains("SENSITIVE_SENTINEL"));
}
#[test]
fn rejects_input_or_configuration_before_network() {
    let expected = get(Err(HttpsError::TransportFailure));
    let mut client = MockHttpsClient::new(vec![expected]);
    let error = retrieve_public(&mut client, "https://evil.example/p/A/").unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::InvalidInput);
    assert!(client.observations().is_empty());
    assert!(client.verify().is_err());
    for invalid in [
        CONFIG.replace("api/graphql", "graphql/query"),
        CONFIG.replace(
            "PolarisLoggedOutDesktopWWWPostRootContentQuery",
            "OtherQuery",
        ),
        CONFIG.replace("27130156389949648", "not-digits"),
        CONFIG.replace("media_id", "shortcode"),
    ] {
        let error = validate_retrieval_config(&invalid).unwrap_err();
        assert_eq!(error.kind, ResolverErrorKind::PolicyDenied);
        assert!(!error.safe_message.contains("graphql"));
    }
}
#[test]
fn stops_after_get_or_page_failure_without_post() {
    for (result, kind, retryable) in [
        (Err(HttpsError::Timeout), ResolverErrorKind::Timeout, true),
        (
            Ok(response(URL, 404, b"missing")),
            ResolverErrorKind::Unavailable,
            false,
        ),
        (
            Ok(response(URL, 429, b"limited")),
            ResolverErrorKind::RateLimited,
            true,
        ),
        (
            Ok(response(URL, 200, b"<html></html>")),
            ResolverErrorKind::MalformedResponse,
            false,
        ),
    ] {
        let mut client = MockHttpsClient::new(vec![get(result)]);
        let error = retrieve_public(&mut client, URL).unwrap_err();
        assert_eq!((error.kind, error.retryable), (kind, retryable));
        assert_eq!(client.observations().len(), 1);
        assert!(client.verify().is_ok());
    }
}
#[test]
fn maps_post_failures_without_retrying_or_leaking() {
    for (result, kind, retryable) in [
        (Err(HttpsError::Timeout), ResolverErrorKind::Timeout, true),
        (
            Err(HttpsError::TransportFailure),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
        (
            Ok(response(
                "https://www.instagram.com/api/graphql",
                403,
                b"SENSITIVE_BODY",
            )),
            ResolverErrorKind::PrivateOrUnavailable,
            false,
        ),
        (
            Ok(response(
                "https://www.instagram.com/api/graphql",
                429,
                b"SENSITIVE_BODY",
            )),
            ResolverErrorKind::RateLimited,
            true,
        ),
        (
            Ok(response(
                "https://www.instagram.com/api/graphql",
                500,
                b"SENSITIVE_BODY",
            )),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
    ] {
        let mut client =
            MockHttpsClient::new(vec![get(Ok(response(URL, 200, &page()))), post(result)]);
        let error = retrieve_public(&mut client, URL).unwrap_err();
        assert_eq!((error.kind, error.retryable), (kind, retryable));
        assert_eq!(client.observations().len(), 2);
        assert!(client.verify().is_ok());
        assert!(!format!("{error:?}").contains("SENSITIVE"));
    }
}
#[test]
fn rejects_lsd_boundaries_before_post() {
    for token in ["", "has space", &"x".repeat(257)] {
        let invalid_page = PAGE.replace("{{EPHEMERAL_LSD}}", token);
        let mut client =
            MockHttpsClient::new(vec![get(Ok(response(URL, 200, invalid_page.as_bytes())))]);
        assert!(retrieve_public(&mut client, URL).is_err());
        assert_eq!(client.observations().len(), 1);
        assert!(client.verify().is_ok());
    }
}

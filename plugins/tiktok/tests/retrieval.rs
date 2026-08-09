// Spec Req 2 bounded request transport scenarios —
// openspec/changes/add-tiktok-resolver/specs/tiktok-video-resolution/spec.md:95-127.
//
// RED state: `retrieve_https` and `retrieve_source` are not yet exported by
// the tiktok crate, so this test file fails to compile (E0432). GREEN lands in
// the next commit when `src/retrieval.rs` issues exactly one headerless-bounded
// GET (accept + accept-language only) and validates the SDK response.

use bex_media_url_resolver_v2::{
    validate_get_request, ExpectedCall, GetRequest, Header, HttpsError, HttpsResponse,
    MockHttpsClient, ResolverErrorKind,
};
use tiktok::{classify_url, retrieve_https, retrieve_source};

const URL: &str = "https://www.tiktok.com/@pokemonlife22/video/7059698374567611694";

/// The exact two request headers the guest is permitted to send, in the
/// canonical (alphabetical) order the plugin emits (`accept` < `accept-language`).
const ACCEPT: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

fn resp(final_url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: final_url.into(),
        headers: vec![],
        body: body.to_vec(),
    }
}

fn expected_get(result: Result<HttpsResponse, HttpsError>) -> ExpectedCall {
    ExpectedCall::Get(
        GetRequest {
            url: URL.into(),
            headers: vec![
                Header {
                    name: "accept".into(),
                    value: ACCEPT.into(),
                },
                Header {
                    name: "accept-language".into(),
                    value: ACCEPT_LANGUAGE.into(),
                },
            ],
        },
        result,
    )
}

// Scenario: Sends GET to canonical TikTok URL (spec.md:103-107)
#[test]
fn sends_get_to_canonical_tiktok_url() {
    let mut client = MockHttpsClient::new(vec![expected_get(Ok(resp(URL, 200, b"")))]);
    let canonical = classify_url(URL).expect("canonical URL admitted");
    let response = retrieve_https(&mut client, &canonical).expect("200 response returns");
    assert_eq!(response.status, 200);
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

// Scenario: Sends only accept and accept-language headers (spec.md:109-112)
#[test]
fn sends_only_accept_and_accept_language_headers() {
    let mut client = MockHttpsClient::new(vec![expected_get(Ok(resp(URL, 200, b"")))]);
    let canonical = classify_url(URL).expect("canonical URL admitted");
    retrieve_https(&mut client, &canonical).expect("bounded GET succeeds");
    // MockHttpsClient matches the expected GetRequest by exact url + header
    // equality; verify() ok proves the guest emitted exactly accept +
    // accept-language in exactly that order, and no other header.
    assert!(client.verify().is_ok());
    assert_eq!(client.observations().len(), 1);
}

// Scenario: Rejects Authorization header from guest (spec.md:114-117)
#[test]
fn rejects_authorization_header_from_guest() {
    let request = GetRequest {
        url: URL.into(),
        headers: vec![Header {
            name: "authorization".into(),
            value: "Bearer secret".into(),
        }],
    };
    assert!(validate_get_request(&request).is_err());
}

// Scenario: Rejects Cookie header from guest (spec.md:119-122)
#[test]
fn rejects_cookie_header_from_guest() {
    let request = GetRequest {
        url: URL.into(),
        headers: vec![Header {
            name: "cookie".into(),
            value: "session=secret".into(),
        }],
    };
    assert!(validate_get_request(&request).is_err());
}

// Scenario: Rejects Referer header from guest (spec.md:124-127)
#[test]
fn rejects_referer_header_from_guest() {
    let request = GetRequest {
        url: URL.into(),
        headers: vec![Header {
            name: "referer".into(),
            value: "https://attacker.example/".into(),
        }],
    };
    assert!(validate_get_request(&request).is_err());
}

// TRIANGULATE — classify-then-retrieve seam: an invalid source URL MUST map to
// InvalidInput with zero host calls, proving the boundary is enforced before
// any transport and exercising the invalid_input helper.
#[test]
fn rejects_invalid_source_with_zero_host_calls() {
    let mut client = MockHttpsClient::new(vec![expected_get(Err(HttpsError::TransportFailure))]);
    let error = retrieve_source(&mut client, "https://evil.example/@user/video/123").unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::InvalidInput);
    assert!(client.observations().is_empty());
    assert!(client.verify().is_err());
}
use bex_media_url_resolver_v2::{ExpectedCall, GetRequest, HttpsResponse, MockHttpsClient};
use instagram::{classify_url, extract_page_state};

#[test]
fn classifies_supported_public_urls_canonically() {
    for (input, expected) in [
        (
            "https://instagram.com/p/ABC_123-",
            "https://www.instagram.com/p/ABC_123-/",
        ),
        (
            "https://www.instagram.com/reel/a/",
            "https://www.instagram.com/reel/a/",
        ),
        (
            "https://www.instagram.com/reels/z9?utm_source=ig_web_copy_link",
            "https://www.instagram.com/reels/z9/",
        ),
        (
            "https://www.instagram.com/tv/x?igsh=MzRlODBiNWFlZA==&utm_medium=copy_link",
            "https://www.instagram.com/tv/x/",
        ),
    ] {
        assert_eq!(classify_url(input).unwrap().as_str(), expected);
    }
}

#[test]
fn rejects_ambiguous_or_unsafe_urls_before_host_use() {
    let rejected = [
        "http://www.instagram.com/p/a/",
        "HTTPS://www.instagram.com/p/a/",
        "https://user@www.instagram.com/p/a/",
        "https://www.instagram.com:443/p/a/",
        "https://evil.instagram.com/p/a/",
        "https://www.instagram.com/p/a/#fragment",
        "https://www.instagram.com/p/a/extra",
        "https://www.instagram.com/p/a%2Fb/",
        "https://www.instagram.com/p/á/",
        "https://www.instagram.com/p/a?unknown=x",
        "https://www.instagram.com/p/a?igsh=x&igsh=y",
        "https://www.instagram.com/p/a?igsh=user@example.com",
        "https://www.instagram.com/p/a?utm_source=",
    ];
    for input in rejected {
        let expected = ExpectedCall::Get(
            GetRequest {
                url: "https://www.instagram.com/p/a/".into(),
                headers: vec![],
            },
            Ok(HttpsResponse {
                status: 200,
                final_url: "https://www.instagram.com/p/a/".into(),
                headers: vec![],
                body: vec![],
            }),
        );
        let mock = MockHttpsClient::new(vec![expected]);
        assert!(classify_url(input).is_err(), "accepted {input}");
        assert!(mock.observations().is_empty());
        assert!(mock.verify().is_err());
    }
}

#[test]
fn enforces_identifier_and_query_boundaries() {
    assert!(classify_url(&format!("https://www.instagram.com/p/{}/", "a".repeat(64))).is_ok());
    assert!(classify_url(&format!("https://www.instagram.com/p/{}/", "a".repeat(65))).is_err());
    assert!(
        classify_url(&format!(
            "https://www.instagram.com/p/a?igsh={}",
            "x".repeat(256)
        ))
        .is_ok()
    );
    assert!(
        classify_url(&format!(
            "https://www.instagram.com/p/a?igsh={}",
            "x".repeat(257)
        ))
        .is_err()
    );
    assert!(classify_url("https://www.instagram.com/p/a?igsh=%2F").is_err());
    assert!(classify_url("https://www.instagram.com/p/a?igsh=x&&utm_source=y").is_err());
}

#[test]
fn extracts_redacted_lsd_and_optional_selected_relay_state() {
    let basic = include_str!("../fixtures/page-basic.html")
        .replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    let state = extract_page_state(basic.as_bytes()).unwrap();
    assert!(!format!("{state:?}").contains("SENSITIVE_SENTINEL"));
    let (lsd, relay) = state.take();
    assert_eq!(format!("{lsd:?}"), "[REDACTED]");
    assert!(relay.is_none());

    let relay_page = include_str!("../fixtures/page-relay.html")
        .replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    let (_, relay) = extract_page_state(relay_page.as_bytes()).unwrap().take();
    assert_eq!(
        relay.as_deref(),
        Some(r#"{"media":{"shortcode":"ABC_123-"}}"#)
    );
}

#[test]
fn extracts_page_state_when_eqmc_marker_carries_a_nonce() {
    let page = include_str!("../fixtures/page-eqmc-nonce.html")
        .replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    let state = extract_page_state(page.as_bytes()).unwrap();
    assert!(!format!("{state:?}").contains("SENSITIVE_SENTINEL"));
    let (lsd, relay) = state.take();
    assert_eq!(format!("{lsd:?}"), "[REDACTED]");
    assert!(relay.is_none());
}

#[test]
fn rejects_missing_duplicate_malformed_or_oversized_page_state() {
    let valid = include_str!("../fixtures/page-basic.html")
        .replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    for page in [
        "<html></html>".to_owned(),
        valid.replace("</html>", &format!("{valid}</html>")),
        valid.replace(r#"{"l":"SENSITIVE_SENTINEL"}"#, "not-json"),
        valid.replace("SENSITIVE_SENTINEL", "has space"),
    ] {
        let error = extract_page_state(page.as_bytes()).unwrap_err();
        assert!(!format!("{error:?}").contains("SENSITIVE_SENTINEL"));
    }
    assert!(extract_page_state(&[0xff]).is_err());

    let relay = include_str!("../fixtures/page-relay.html")
        .replace("{{EPHEMERAL_LSD}}", "SENSITIVE_SENTINEL");
    assert!(extract_page_state(format!("{relay}{relay}").as_bytes()).is_err());
    let huge = format!(
        "<script id=\"__eqmc\" type=\"application/json\">{{\"l\":\"SENSITIVE_SENTINEL\"}}</script><script type=\"application/json\" data-sjs=\"RelayPrefetchedStreamCache\">{{\"x\":\"{}\"}}</script>",
        "x".repeat(65_536)
    );
    assert!(extract_page_state(huge.as_bytes()).is_err());
}

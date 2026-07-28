use bex_media_url_resolver_v2::{
    EphemeralLsd, GetRequest, Header, HttpsResponse, PublicGraphqlRequest, validate_get_request,
    validate_https_response, validate_public_graphql_request,
};
fn get(url: &str) -> GetRequest {
    GetRequest {
        url: url.into(),
        headers: vec![Header {
            name: "accept".into(),
            value: "text/html".into(),
        }],
    }
}
fn response() -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: "https://www.instagram.com/p/ABC123/".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "text/html".into(),
        }],
        body: b"ok".to_vec(),
    }
}
fn post() -> PublicGraphqlRequest {
    PublicGraphqlRequest {
        url: "https://www.instagram.com/api/graphql".into(),
        lsd: "SENSITIVE_SENTINEL".into(),
        friendly_name: "PolarisLoggedOutDesktopWWWPostRootContentQuery".into(),
        doc_id: "27130156389949648".into(),
        variables: r#"{"media_id":"123"}"#.into(),
    }
}
#[test]
fn redacts_ephemeral_lsd() {
    let value = EphemeralLsd::new("SENSITIVE_SENTINEL".into()).expect("valid LSD");
    for rendered in [format!("{value}"), format!("{value:?}")] {
        assert!(!rendered.contains("SENSITIVE_SENTINEL"));
        assert_eq!(rendered, "[REDACTED]");
    }
    assert!(EphemeralLsd::new("x".repeat(257)).is_err());
    assert!(EphemeralLsd::new("has space".into()).is_err());
}
#[test]
fn validates_exact_get_policy() {
    assert!(validate_get_request(&get("https://www.instagram.com/reel/ABC_123-/")).is_ok());
    for url in [
        "http://www.instagram.com/p/a/",
        "https://evil.instagram.com/p/a/",
        "https://instagram.com.example.org/p/a/",
        "https://user@instagram.com/p/a/",
        "https://instagram.com:443/p/a/",
        "https://instagram.com/p/a/#fragment",
        "https://instagram.com/profile/",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "accepted {url}");
    }
    let mut duplicate = get("https://instagram.com/p/a/");
    duplicate.headers.push(Header {
        name: "Accept".into(),
        value: "other".into(),
    });
    assert!(validate_get_request(&duplicate).is_err());
    let mut forbidden = get("https://instagram.com/p/a/");
    forbidden.headers[0].name = "cookie".into();
    assert!(validate_get_request(&forbidden).is_err());
    forbidden.headers[0] = Header {
        name: "accept".into(),
        value: "ok\r\ncookie: x".into(),
    };
    assert!(validate_get_request(&forbidden).is_err());
}
#[test]
fn validates_exact_bandcamp_get_policy() {
    assert!(
        validate_get_request(&get(
            "https://relapsealumni.bandcamp.com/track/hail-to-fire"
        ))
        .is_ok()
    );
    assert!(validate_get_request(&get("https://a.bandcamp.com/track/x")).is_ok());
    for url in [
        "http://a.bandcamp.com/track/x",
        "https://www.bandcamp.com/track/x",
        "https://bandcamp.com/track/x",
        "https://a.bandcamp.com/album/x",
        "https://a.bandcamp.com/track/",
        "https://a.bandcamp.com/track/x?q=1",
        "https://a.bandcamp.com/track/x#f",
        "https://user@a.bandcamp.com/track/x",
        "https://a.bandcamp.com:443/track/x",
        "https://a.bandcamp.com/track/x/extra",
        "https://A.bandcamp.com/track/x",
        "https://a.bandcamp.com/track/X",
        "https://a-.bandcamp.com/track/x",
        "https://evil.bandcamp.com.example/track/x",
        "https://a.bandcamp.com.example.org/track/x",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "accepted {url}");
    }
}

#[test]
fn validates_bandcamp_final_url_in_response() {
    let mut response = HttpsResponse {
        status: 200,
        final_url: "https://a.bandcamp.com/track/x".into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "text/html".into(),
        }],
        body: b"ok".to_vec(),
    };
    assert!(validate_https_response(&response).is_ok());
    response.final_url = "https://other.bandcamp.com/track/y".into();
    assert!(validate_https_response(&response).is_ok());
    // off-origin redirect to a non-page host is rejected
    response.final_url = "https://t4.bcbits.com/stream/abc".into();
    assert!(validate_https_response(&response).is_err());
    response.final_url = "https://evil.example/track/x".into();
    assert!(validate_https_response(&response).is_err());
}

#[test]
fn validates_graphql_and_response_boundaries() {
    assert!(validate_public_graphql_request(&post()).is_ok());
    let mut invalid = post();
    invalid.doc_id = "not-digits".into();
    assert!(validate_public_graphql_request(&invalid).is_err());
    invalid = post();
    invalid.variables = "[]".into();
    assert!(validate_public_graphql_request(&invalid).is_err());
    assert!(validate_https_response(&response()).is_ok());
    let mut invalid_response = response();
    invalid_response.status = 99;
    assert!(validate_https_response(&invalid_response).is_err());
    invalid_response = response();
    invalid_response.headers[0].name = "set-cookie".into();
    assert!(validate_https_response(&invalid_response).is_err());
    invalid_response = response();
    invalid_response.body = vec![0; 4_194_305];
    assert!(validate_https_response(&invalid_response).is_err());
}

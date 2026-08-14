use bex_media_url_resolver_v2::{
    EphemeralFbDtsg, EphemeralLsd, ExpectedCall, GetRequest, HttpsClient, HttpsError,
    HttpsResponse, MockHttpsClient, PublicGraphqlExpectation, TahoeExpectation,
    build_graphql_request, build_tahoe_request,
};

fn response(body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: "https://www.instagram.com/p/a/".into(),
        headers: vec![],
        body: body.into(),
    }
}
fn get() -> GetRequest {
    GetRequest {
        url: "https://www.instagram.com/p/a/".into(),
        headers: vec![],
    }
}
fn graphql(lsd: &str) -> bex_media_url_resolver_v2::PublicGraphqlCall {
    build_graphql_request(
        "https://www.instagram.com/api/graphql",
        EphemeralLsd::new(lsd.into()).unwrap(),
        "friendly",
        "123",
        "{}",
    )
    .unwrap()
}

#[test]
fn replays_calls_in_order_without_sensitive_observations() {
    let expected = PublicGraphqlExpectation::new(
        "https://www.instagram.com/api/graphql",
        "friendly",
        "123",
        "{}",
        18,
        Ok(response(b"post")),
    );
    let mut mock = MockHttpsClient::new(vec![
        ExpectedCall::Get(get(), Ok(response(b"get"))),
        ExpectedCall::Graphql(expected),
    ]);
    assert_eq!(mock.get(get()).unwrap().body, b"get");
    assert_eq!(
        mock.post_public_graphql(graphql("SENSITIVE_SENTINEL"))
            .unwrap()
            .body,
        b"post"
    );
    assert!(mock.verify().is_ok());
    assert_eq!(
        mock.observations()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>(),
        ["get", "post-public-graphql"]
    );
    assert!(!format!("{:?}", mock.observations()).contains("SENSITIVE_SENTINEL"));
}

#[test]
fn fails_closed_on_reordered_mismatched_or_unconsumed_calls() {
    let mut reordered = MockHttpsClient::new(vec![ExpectedCall::Get(get(), Ok(response(b"ok")))]);
    assert!(matches!(
        reordered.post_public_graphql(graphql("x")),
        Err(HttpsError::InvalidRequest)
    ));
    assert!(reordered.verify().is_err());

    let expected = PublicGraphqlExpectation::new(
        "https://www.instagram.com/api/graphql",
        "friendly",
        "123",
        "{}",
        1,
        Ok(response(b"ok")),
    );
    let unconsumed = MockHttpsClient::new(vec![ExpectedCall::Graphql(expected)]);
    assert!(unconsumed.verify().is_err());
}

#[test]
fn observations_never_retain_query_values() {
    let request = || GetRequest {
        url: "https://www.instagram.com/p/a/?token=SENSITIVE_QUERY".into(),
        headers: vec![],
    };
    let mut mock = MockHttpsClient::new(vec![ExpectedCall::Get(request(), Ok(response(b"ok")))]);
    assert!(mock.get(request()).is_ok());
    assert!(!format!("{:?}", mock.observations()).contains("SENSITIVE_QUERY"));
}

#[test]
fn replays_typed_failures_deterministically() {
    let mut first = MockHttpsClient::new(vec![ExpectedCall::Get(get(), Err(HttpsError::Timeout))]);
    let mut second = MockHttpsClient::new(vec![ExpectedCall::Get(get(), Err(HttpsError::Timeout))]);
    assert!(matches!(first.get(get()), Err(HttpsError::Timeout)));
    assert!(matches!(second.get(get()), Err(HttpsError::Timeout)));
    assert_eq!(first.observations(), second.observations());
}

fn tahoe_response(body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: "https://www.facebook.com/video/tahoe/async/10107927396957931/".into(),
        headers: vec![],
        body: body.into(),
    }
}

fn tahoe(fb_dtsg: &str) -> bex_media_url_resolver_v2::TahoeCall {
    build_tahoe_request(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        EphemeralFbDtsg::new(fb_dtsg.into()).unwrap(),
        "PHASED:DEFAULT",
        "123456",
    )
    .unwrap()
}

#[test]
fn tahoe_call_matches_expectation_and_replays_deterministically() {
    let expected = TahoeExpectation::new(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        15,
        "PHASED:DEFAULT",
        "123456",
        Ok(tahoe_response(b"tahoe")),
    );
    let mut mock = MockHttpsClient::new(vec![ExpectedCall::Tahoe(expected)]);
    assert_eq!(
        mock.post_tahoe(tahoe("SENSITIVE_TOKEN")).unwrap().body,
        b"tahoe"
    );
    assert!(mock.verify().is_ok());
    assert_eq!(
        mock.observations()
            .iter()
            .map(|item| item.operation)
            .collect::<Vec<_>>(),
        ["post-tahoe"]
    );
    assert!(!format!("{:?}", mock.observations()).contains("SENSITIVE_TOKEN"));
}

#[test]
fn tahoe_call_fails_closed_on_mismatched_or_unconsumed() {
    // Mismatch: wrong URL
    let expected = TahoeExpectation::new(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        15,
        "PHASED:DEFAULT",
        "123456",
        Ok(tahoe_response(b"ok")),
    );
    let mut mock = MockHttpsClient::new(vec![ExpectedCall::Tahoe(expected)]);
    let mismatched = build_tahoe_request(
        "https://www.facebook.com/video/tahoe/async/99999999/",
        EphemeralFbDtsg::new("SENSITIVE_TOKEN".into()).unwrap(),
        "PHASED:DEFAULT",
        "123456",
    )
    .unwrap();
    assert!(matches!(
        mock.post_tahoe(mismatched),
        Err(HttpsError::InvalidRequest)
    ));
    assert!(mock.verify().is_err());

    // Unconsumed: Tahoe expectation remains unspent
    let unconsumed_expected = TahoeExpectation::new(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        15,
        "PHASED:DEFAULT",
        "123456",
        Ok(tahoe_response(b"ok")),
    );
    let unconsumed = MockHttpsClient::new(vec![ExpectedCall::Tahoe(unconsumed_expected)]);
    assert!(unconsumed.verify().is_err());
}

#[test]
fn tahoe_call_mismatch_on_field_values() {
    // Correct URL + fb_dtsg_len, wrong pkg_cohort → reject
    let expected = TahoeExpectation::new(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        15,
        "PHASED:DEFAULT",
        "123456",
        Ok(tahoe_response(b"ok")),
    );
    let mut mock = MockHttpsClient::new(vec![ExpectedCall::Tahoe(expected)]);
    let wrong_cohort = build_tahoe_request(
        "https://www.facebook.com/video/tahoe/async/10107927396957931/",
        EphemeralFbDtsg::new("SENSITIVE_TOKEN".into()).unwrap(),
        "PHASED:OTHER",
        "123456",
    )
    .unwrap();
    assert!(matches!(
        mock.post_tahoe(wrong_cohort),
        Err(HttpsError::InvalidRequest)
    ));
}

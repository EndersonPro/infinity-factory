use bex_media_url_resolver_v2::{
    EphemeralLsd, ExpectedCall, GetRequest, HttpsClient, HttpsError, HttpsResponse,
    MockHttpsClient, PublicGraphqlExpectation, build_graphql_request,
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

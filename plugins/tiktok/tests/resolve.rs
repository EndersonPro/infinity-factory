// Composition: classify -> retrieve -> parse -> map (design.md data flow).
// End-to-end exercise of resolve_public_at against a hermetic MockHttpsClient
// seeded with the committed tt_v3.html probe body. No live network.

use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, Header, HttpsResponse, MockHttpsClient, Resolution,
};
use tiktok::resolve_public_at;

const URL: &str = "https://www.tiktok.com/@pokemonlife22/video/7059698374567611694";
const ACCEPT: &str = "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8";
const ACCEPT_LANGUAGE: &str = "en-US,en;q=0.9";

fn fixture_body() -> Vec<u8> {
    std::fs::read("tests/fixtures/tt_v3.html").expect("tt_v3.html fixture present")
}

#[test]
fn resolves_canonical_probe_end_to_end_into_candidates() {
    let body = fixture_body();
    let mut client = MockHttpsClient::new(vec![ExpectedCall::Get(
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
        Ok(HttpsResponse {
            status: 200,
            final_url: URL.into(),
            headers: vec![],
            body,
        }),
    )]);
    let response = resolve_public_at(&mut client, URL).expect("probe resolves end-to-end");
    assert!(response.metadata.is_none(), "tiktok carries no metadata");
    let items = match response.resolution {
        Resolution::Candidates(items) => items,
        other => panic!("expected candidates, got {other:?}"),
    };
    assert!(
        (2..=16).contains(&items.len()),
        "candidates within sdk bound"
    );
    assert!(
        items.iter().all(|c| !c
            .stream
            .url
            .starts_with("https://www.tiktok.com/aweme/v1/play/")),
        "gateway rejected from output"
    );
    assert_eq!(client.observations().len(), 1);
    assert!(client.verify().is_ok());
}

#[test]
fn rejects_non_canonical_source_before_any_host_call() {
    let mut client = MockHttpsClient::new(vec![]);
    let error = resolve_public_at(&mut client, "https://vm.tiktok.com/Z123/").unwrap_err();
    assert_eq!(
        error.kind,
        bex_media_url_resolver_v2::ResolverErrorKind::InvalidInput
    );
    assert!(client.observations().is_empty());
}

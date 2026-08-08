//! Parsing and mapping against bodies captured from the live endpoint on
//! 2026-08-07. Committed verbatim so a schema drift shows up here rather than
//! on a device.

use bex_media_url_resolver_v2::{Resolution, ResolverErrorKind, validate_resolver_response};
use x::parse_and_map;

const MULTI: &[u8] = include_bytes!("fixtures/video_multi_variant.json");
const SINGLE: &[u8] = include_bytes!("fixtures/video_single_ish.json");
const TEXT_ONLY: &[u8] = include_bytes!("fixtures/text_only.json");
const TOMBSTONE: &[u8] = include_bytes!("fixtures/tombstone.json");

fn candidates(body: &[u8]) -> Vec<(String, String, Option<String>)> {
    let response = parse_and_map(body).expect("parses");
    validate_resolver_response(&response).expect("within compatibility limits");
    match response.resolution {
        Resolution::Candidates(items) => items
            .into_iter()
            .map(|item| (item.id, item.stream.url, item.stream.quality_label))
            .collect(),
        Resolution::Direct(stream) => {
            vec![("direct".to_owned(), stream.url, stream.quality_label)]
        }
        other => panic!("expected streams, got {other:?}"),
    }
}

#[test]
fn maps_a_post_with_several_renditions() {
    let found = candidates(MULTI);
    assert!(found.len() > 1, "expected candidates, got {}", found.len());
    assert!(found.len() <= 16);
}

#[test]
fn returns_only_progressive_mp4_on_the_media_host() {
    for body in [MULTI, SINGLE] {
        for (id, url, _) in candidates(body) {
            assert!(url.starts_with("https://video.twimg.com/"), "{id}: {url}");
            assert!(!url.contains(".m3u8"), "{id}: HLS leaked into the result");
            assert!(!id.is_empty());
        }
    }
}

/// Highest bitrate first, so a caller taking the head takes the best. The
/// labels come out of the rendition path, which is where the only size
/// information in the payload lives.
#[test]
fn orders_renditions_by_descending_quality() {
    let found = candidates(SINGLE);
    let heights: Vec<u32> = found
        .iter()
        .filter_map(|(_, _, label)| label.as_ref())
        .filter_map(|label| label.split_once('x'))
        .filter_map(|(_, height)| height.parse().ok())
        .collect();
    assert!(heights.len() >= 2, "expected labelled renditions");
    assert!(
        heights.windows(2).all(|pair| pair[0] >= pair[1]),
        "not descending: {heights:?}"
    );
}

#[test]
fn gives_every_candidate_a_distinct_id() {
    let found = candidates(MULTI);
    let mut ids: Vec<&str> = found.iter().map(|(id, _, _)| id.as_str()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "duplicate candidate id");
}

#[test]
fn carries_metadata_without_leaking_the_body() {
    let response = parse_and_map(SINGLE).expect("parses");
    let metadata = response.metadata.expect("metadata");
    assert!(metadata.title.is_some_and(|title| title.len() <= 256));
    assert!(metadata.author.is_some_and(|author| author.len() <= 128));
    assert!(metadata.duration_milliseconds.is_some_and(|ms| ms > 0));
    assert!(
        metadata
            .thumbnail_url
            .is_some_and(|url| url.starts_with("https://pbs.twimg.com/"))
    );
}

/// The common answer, and not a failure: the link was good, the fetch worked,
/// and there is simply nothing to download.
#[test]
fn reports_a_post_without_video_as_unsupported() {
    let response = parse_and_map(TEXT_ONLY).expect("parses");
    validate_resolver_response(&response).expect("within compatibility limits");
    assert!(matches!(response.resolution, Resolution::Unsupported(_)));
    assert!(response.metadata.is_none());
}

#[test]
fn reports_a_withdrawn_post_as_unavailable() {
    let error = parse_and_map(TOMBSTONE).expect_err("tombstone is not resolvable");
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert!(!error.retryable);
}

#[test]
fn refuses_a_body_that_is_not_the_expected_document() {
    for body in [b"".as_slice(), b"not json", b"[]", b"null", b"{"] {
        let error = parse_and_map(body).expect_err("malformed");
        assert_eq!(error.kind, ResolverErrorKind::MalformedResponse, "{body:?}");
    }
}

/// No safe message may carry the post text, a URL, or anything else lifted out
/// of the payload -- the app shows a generic sentence and the detail goes to
/// its crash reporter, not to the screen.
#[test]
fn keeps_payload_content_out_of_error_messages() {
    for body in [TOMBSTONE, b"not json".as_slice()] {
        let message = parse_and_map(body).expect_err("error").safe_message;
        assert!(!message.contains("http"), "{message}");
        assert!(message.len() <= 256);
    }
}

//! Transport admission for the X syndication endpoint.
//!
//! A new file rather than an addition to `boundaries.rs`: the acceptance
//! condition for this change is that no pre-existing test file is edited, so
//! the regression evidence stays a `git diff` rather than a judgement call.

use bex_media_url_resolver_v2::{
    GetRequest, Header, HttpsResponse, validate_get_request, validate_https_response,
};

const CANONICAL: &str =
    "https://cdn.syndication.twimg.com/tweet-result?id=1163218564784017422&token=2ticx4q85hf";

fn get(url: &str) -> GetRequest {
    GetRequest {
        url: url.into(),
        headers: vec![],
    }
}

fn response(final_url: &str) -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: final_url.into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "application/json".into(),
        }],
        body: b"{}".to_vec(),
    }
}

#[test]
fn admits_the_canonical_syndication_request() {
    assert!(validate_get_request(&get(CANONICAL)).is_ok());
}

#[test]
fn admits_the_canonical_syndication_final_url() {
    assert!(validate_https_response(&response(CANONICAL)).is_ok());
}

#[test]
fn admits_the_bounds_of_each_field() {
    for url in [
        "https://cdn.syndication.twimg.com/tweet-result?id=1&token=a",
        "https://cdn.syndication.twimg.com/tweet-result?id=1234567890123456789&token=abcdefghij123456",
    ] {
        assert!(validate_get_request(&get(url)).is_ok(), "{url}");
    }
}

/// The host is compared as an exact literal, so every neighbouring spelling is
/// a different host and none of them is admitted.
#[test]
fn refuses_look_alike_authorities() {
    for authority in [
        "cdn.syndication.twimg.com.evil.tld",
        "evil.cdn.syndication.twimg.com",
        "cdn-syndication.twimg.com",
        "syndication.twimg.com",
        "twimg.com",
        "cdn.syndication.twimg.com.",
        "CDN.SYNDICATION.TWIMG.COM",
    ] {
        let url =
            format!("https://{authority}/tweet-result?id=1163218564784017422&token=2ticx4q85hf");
        assert!(validate_get_request(&get(&url)).is_err(), "{authority}");
    }
}

/// These are refused by `parsed_https` for every authority. Asserted here
/// rather than re-implemented in the syndication branch, so the branch stays
/// the smallest thing that can be correct.
#[test]
fn refuses_what_the_shared_parser_already_refuses() {
    for url in [
        "http://cdn.syndication.twimg.com/tweet-result?id=1&token=a",
        "https://user@cdn.syndication.twimg.com/tweet-result?id=1&token=a",
        "https://cdn.syndication.twimg.com:8443/tweet-result?id=1&token=a",
        "https://cdn.syndication.twimg.com/tweet-result?id=1&token=a#frag",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "{url}");
    }
}

#[test]
fn refuses_non_canonical_paths() {
    for path in [
        "/tweet-result/",
        "/tweet-result/extra",
        "/Tweet-Result",
        "/tweet-results",
        "/",
        "/tweet-result/../../evil",
        "/evil/../tweet-result/extra",
    ] {
        let url = format!("https://cdn.syndication.twimg.com{path}?id=1&token=a");
        assert!(validate_get_request(&get(&url)).is_err(), "{path}");
    }
}

/// Traversal is resolved by the parser before any branch sees the path, for
/// every authority alike. `/../tweet-result` normalises to `/tweet-result`, so
/// what would go on the wire is byte-identical to the canonical request and
/// admitting it grants nothing extra.
///
/// The case that matters is the other direction, and it is covered above:
/// `/tweet-result/../../evil` normalises to `/evil` and is refused.
#[test]
fn admits_traversal_that_normalises_back_to_the_canonical_path() {
    let url = "https://cdn.syndication.twimg.com/../tweet-result?id=1&token=a";
    assert!(validate_get_request(&get(url)).is_ok());
}

#[test]
fn refuses_malformed_queries() {
    for query in [
        "",
        "?id=1163218564784017422",
        "?token=2ticx4q85hf",
        "?token=2ticx4q85hf&id=1163218564784017422",
        "?id=1&token=a&extra=b",
        "?id=1&id=2&token=a",
        "?id=&token=a",
        "?id=1&token=",
        "?id=01163218564784017422&token=a",
        "?id=abc&token=a",
        "?id=12345678901234567890&token=a",
        "?id=1&token=UPPER",
        "?id=1&token=has0zero",
        "?id=1&token=has-hyphen",
        "?id=1&token=abcdefghijklmnopq",
    ] {
        let url = format!("https://cdn.syndication.twimg.com/tweet-result{query}");
        assert!(validate_get_request(&get(&url)).is_err(), "{query:?}");
    }
}

/// The branch grants a host, never a header. The User-Agent stays host-owned:
/// the endpoint was measured to answer identically with and without an
/// override, so yt-dlp's `Googlebot` is vestigial and buys no exception.
#[test]
fn refuses_extra_request_headers() {
    for name in ["user-agent", "authorization", "x-guest-token", "cookie"] {
        let request = GetRequest {
            url: CANONICAL.into(),
            headers: vec![Header {
                name: name.into(),
                value: "value".into(),
            }],
        };
        assert!(validate_get_request(&request).is_err(), "{name}");
    }
}

/// The acceptance condition of this change, stated as a test rather than left
/// to the suite count: the three existing authorities behave exactly as before.
#[test]
fn leaves_the_existing_authorities_admitted() {
    for url in [
        "https://www.instagram.com/p/ABC123/",
        "https://artist.bandcamp.com/track/some-song",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    ] {
        assert!(validate_get_request(&get(url)).is_ok(), "{url}");
    }
}

/// The syndication query shape must not have leaked into the other branches:
/// Instagram and Bandcamp forbid a query outright, and YouTube admits only
/// `v=`.
#[test]
fn leaves_the_existing_authorities_refusing_what_they_refused() {
    for url in [
        "https://www.instagram.com/p/ABC123/?id=1&token=a",
        "https://artist.bandcamp.com/track/some-song?id=1&token=a",
        "https://www.youtube.com/watch?id=1&token=a",
        "https://cdn.syndication.twimg.com/p/ABC123/",
        "https://cdn.syndication.twimg.com/track/some-song",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "{url}");
    }
}

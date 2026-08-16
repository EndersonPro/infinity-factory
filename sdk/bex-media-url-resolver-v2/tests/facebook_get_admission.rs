//! Shared-SDK admission boundary for canonical public Facebook page GETs.

use bex_media_url_resolver_v2::{
    GetRequest, Header, HttpsResponse, validate_get_request, validate_https_response,
};

const CANONICAL_FORMS: [&str; 5] = [
    "https://www.facebook.com/zuck/videos/10107927396957931/",
    "https://www.facebook.com/zuck/reels/10107927396957931/",
    "https://www.facebook.com/zuck/reel/10107927396957931/",
    "https://www.facebook.com/reel/10107927396957931/",
    "https://www.facebook.com/watch/?v=10107927396957931",
];

fn get(url: &str) -> GetRequest {
    GetRequest {
        url: url.into(),
        headers: vec![Header {
            name: "accept".into(),
            value: "text/html".into(),
        }],
    }
}

fn response(final_url: &str) -> HttpsResponse {
    HttpsResponse {
        status: 200,
        final_url: final_url.into(),
        headers: vec![Header {
            name: "content-type".into(),
            value: "text/html".into(),
        }],
        body: b"ok".to_vec(),
    }
}

#[test]
fn admits_all_canonical_facebook_get_forms() {
    for url in CANONICAL_FORMS {
        assert!(validate_get_request(&get(url)).is_ok(), "rejected {url}");
    }
}

#[test]
fn admits_all_canonical_facebook_final_urls() {
    for url in CANONICAL_FORMS {
        assert!(
            validate_https_response(&response(url)).is_ok(),
            "rejected {url}"
        );
    }
}

#[test]
fn admits_facebook_identifier_and_user_boundaries() {
    let longest_user = "a".repeat(64);
    let longest_pfbid = format!("pfbid{}", "A".repeat(59));
    for url in [
        "https://www.facebook.com/a/videos/1/".into(),
        format!(
            "https://www.facebook.com/{longest_user}/videos/{}/",
            "1".repeat(64)
        ),
        "https://www.facebook.com/user.name_1-2/reels/pfbidA/".into(),
        format!("https://www.facebook.com/zuck/reel/{longest_pfbid}/"),
    ] {
        assert!(validate_get_request(&get(&url)).is_ok(), "rejected {url}");
    }
}

/// The `web` mobile-web hop Facebook's edge bounces a logged-out GET through
/// (verified live) is an equally canonical authority to `www`, not a
/// look-alike host.
#[test]
fn admits_the_web_facebook_authority_alongside_www() {
    for url in CANONICAL_FORMS.map(|url| url.replacen("www.facebook.com", "web.facebook.com", 1)) {
        assert!(validate_get_request(&get(&url)).is_ok(), "rejected {url}");
        assert!(
            validate_https_response(&response(&url)).is_ok(),
            "rejected {url}"
        );
    }
}

#[test]
fn refuses_non_exact_facebook_authorities() {
    for authority in [
        "facebook.com",
        "m.facebook.com",
        "mobile.facebook.com",
        "www.facebook.com.evil.test",
        "evil.www.facebook.com",
        "WWW.FACEBOOK.COM",
        "www.facebook.com.",
    ] {
        let url = format!("https://{authority}/zuck/videos/10107927396957931/");
        assert!(validate_get_request(&get(&url)).is_err(), "admitted {url}");
    }
}

#[test]
fn refuses_noncanonical_facebook_paths_and_identifiers() {
    for url in [
        "https://www.facebook.com/zuck/videos/10107927396957931",
        "https://www.facebook.com/zuck/videos//",
        "https://www.facebook.com/zuck/videos/not-a-number/",
        "https://www.facebook.com/zuck/videos/pfbid/",
        "https://www.facebook.com/zuck/videos/pfbid+/",
        "https://www.facebook.com/bad!user/videos/1/",
        "https://www.facebook.com//videos/1/",
        "https://www.facebook.com/zuck/posts/1/",
        "https://www.facebook.com/video.php?v=1",
        "https://www.facebook.com/watch/",
        "https://www.facebook.com/reel/1/extra",
        "https://www.facebook.com/reel/1/?q=1",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "admitted {url}");
    }
}

#[test]
fn refuses_invalid_facebook_url_components() {
    for url in [
        "http://www.facebook.com/zuck/videos/1/",
        "https://user@www.facebook.com/zuck/videos/1/",
        "https://www.facebook.com:443/zuck/videos/1/",
        "https://www.facebook.com/zuck/videos/1/#fragment",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "admitted {url}");
    }
}

#[test]
fn refuses_noncanonical_watch_queries() {
    for url in [
        "https://www.facebook.com/watch/",
        "https://www.facebook.com/watch/?v=",
        "https://www.facebook.com/watch/?v=abc",
        "https://www.facebook.com/watch/?v=1&extra=2",
        "https://www.facebook.com/watch/?extra=2&v=1",
        "https://www.facebook.com/watch/?v=1&v=2",
        "https://www.facebook.com/zuck/videos/1/?v=1",
    ] {
        assert!(validate_get_request(&get(url)).is_err(), "admitted {url}");
    }
}

#[test]
fn refuses_forbidden_facebook_request_headers() {
    for name in ["authorization", "cookie", "referer", "user-agent"] {
        let request = GetRequest {
            url: CANONICAL_FORMS[0].into(),
            headers: vec![Header {
                name: name.into(),
                value: "value".into(),
            }],
        };
        assert!(validate_get_request(&request).is_err(), "admitted {name}");
    }
}

#[test]
fn retains_representative_existing_platform_policy() {
    assert!(validate_get_request(&get("https://www.instagram.com/p/ABC123/")).is_ok());
    assert!(validate_get_request(&get("https://www.youtube.com/watch?v=dQw4w9WgXcQ")).is_ok());
    assert!(validate_get_request(&get("https://www.instagram.com/p/ABC123/?v=1")).is_err());
    assert!(validate_get_request(&get("https://www.youtube.com/watch?v=too-short")).is_err());
}

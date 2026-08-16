use facebook::{CanonicalUrl, classify_url};

fn ok(input: &str, expected_canonical: &str, expected_id: &str) {
    let url: CanonicalUrl = classify_url(input).expect("admitted");
    assert_eq!(url.as_str(), expected_canonical, "canonical for {input}");
    assert_eq!(url.video_id(), expected_id, "video id for {input}");
}

#[test]
fn admits_canonical_videos_watch_reels_and_reel_shapes() {
    ok(
        "https://www.facebook.com/zuck/videos/10107927396957931/",
        "https://www.facebook.com/zuck/videos/10107927396957931/",
        "10107927396957931",
    );
    ok(
        "https://www.facebook.com/watch/?v=10107927396957931",
        "https://www.facebook.com/watch/?v=10107927396957931",
        "10107927396957931",
    );
    // `/watch?v=` without the trailing slash is admitted and canonicalised
    // to the slash form (yt-dlp `facebook.py` makes the slash optional).
    ok(
        "https://www.facebook.com/watch?v=10107927396957931",
        "https://www.facebook.com/watch/?v=10107927396957931",
        "10107927396957931",
    );
    ok(
        "https://www.facebook.com/zuck/reels/10107927396957931/",
        "https://www.facebook.com/zuck/reels/10107927396957931/",
        "10107927396957931",
    );
    // reel with a user segment (spec Req 1 `{user}/reel/{id}/`)
    ok(
        "https://www.facebook.com/zuck/reel/10107927396957931/",
        "https://www.facebook.com/zuck/reel/10107927396957931/",
        "10107927396957931",
    );
    // reel without a user segment (real-world `/reel/{id}/`)
    ok(
        "https://www.facebook.com/reel/10107927396957931/",
        "https://www.facebook.com/reel/10107927396957931/",
        "10107927396957931",
    );
    // apex `facebook.com` is admitted and canonicalised to www
    ok(
        "https://facebook.com/zuck/videos/10107927396957931/",
        "https://www.facebook.com/zuck/videos/10107927396957931/",
        "10107927396957931",
    );
    // `web.facebook.com` is the mobile-web hop the host's edge bounces a
    // logged-out GET through (verified live) and is preserved, not rewritten.
    ok(
        "https://web.facebook.com/zuck/videos/10107927396957931/",
        "https://web.facebook.com/zuck/videos/10107927396957931/",
        "10107927396957931",
    );
    ok(
        "https://web.facebook.com/watch/?v=10107927396957931",
        "https://web.facebook.com/watch/?v=10107927396957931",
        "10107927396957931",
    );
}

#[test]
fn admits_pfbid_base64_identifier() {
    ok(
        "https://www.facebook.com/zuck/videos/pfbid0AbCdEfGhIjKlMnOpQrSt/",
        "https://www.facebook.com/zuck/videos/pfbid0AbCdEfGhIjKlMnOpQrSt/",
        "pfbid0AbCdEfGhIjKlMnOpQrSt",
    );
}

#[test]
fn rejects_out_of_scope_or_unsafe_urls_before_any_host_call() {
    let rejected = [
        // scheme / transport shape
        "http://www.facebook.com/zuck/videos/10107927396957931/",
        "facebook:10107927396957931",
        "HTTPS://www.facebook.com/zuck/videos/10107927396957931/",
        // non-canonical hosts (`web.facebook.com` is admitted — see
        // `admits_canonical_videos_watch_reels_and_reel_shapes`)
        "https://m.facebook.com/watch/?v=10107927396957931",
        "https://mbasic.facebook.com/watch/?v=10107927396957931",
        "https://evil.facebook.com/zuck/videos/10107927396957931/",
        // userinfo / port / fragment
        "https://user@www.facebook.com/zuck/videos/10107927396957931/",
        "https://:pass@www.facebook.com/zuck/videos/10107927396957931/",
        "https://www.facebook.com:443/zuck/videos/10107927396957931/",
        "https://www.facebook.com/watch/?v=10107927396957931#t=1",
        // out-of-scope paths
        "https://www.facebook.com/zuck/posts/10107927396957931",
        "https://www.facebook.com/groups/foo/permalink/10107927396957931/",
        "https://www.facebook.com/stories/10107927396957931/",
        "https://www.facebook.com/video.php?v=10107927396957931",
        "https://www.facebook.com/plugins/video.php?href=x",
        "https://www.facebook.com/zuck/videos/10107927396957931/embed",
        // query on path-style URLs, or malformed watch query
        "https://www.facebook.com/zuck/videos/10107927396957931/?ref=share",
        "https://www.facebook.com/watch/?v=10107927396957931&ref=share",
        "https://www.facebook.com/watch/?v=",
        "https://www.facebook.com/watch/?id=10107927396957931",
        // bad user / bad id
        "https://www.facebook.com//videos/10107927396957931/",
        "https://www.facebook.com/zuck/videos/",
        "https://www.facebook.com/zuck/videos/pfbid/",
        // non-ascii
        "https://www.facebook.com/zúck/videos/10107927396957931/",
    ];
    for input in rejected {
        assert!(classify_url(input).is_err(), "accepted {input}");
    }
}

#[test]
fn enforces_user_and_identifier_length_boundaries() {
    let max_user = "a".repeat(64);
    assert!(
        classify_url(&format!(
            "https://www.facebook.com/{max_user}/videos/10107927396957931/"
        ))
        .is_ok()
    );
    let over_user = "a".repeat(65);
    assert!(
        classify_url(&format!(
            "https://www.facebook.com/{over_user}/videos/10107927396957931/"
        ))
        .is_err()
    );
    let max_id = "9".repeat(64);
    assert!(classify_url(&format!("https://www.facebook.com/zuck/videos/{max_id}/")).is_ok());
    let over_id = "9".repeat(65);
    assert!(classify_url(&format!("https://www.facebook.com/zuck/videos/{over_id}/")).is_err());
    assert!(classify_url(&format!("https://www.facebook.com/watch/?v={max_id}")).is_ok());
    assert!(classify_url(&format!("https://www.facebook.com/watch/?v={over_id}")).is_err());
}

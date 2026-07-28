use bandcamp::classify_url;

#[test]
fn classifies_canonical_public_track_urls() {
    for input in [
        "https://relapsealumni.bandcamp.com/track/hail-to-fire",
        "https://youtube-dl.bandcamp.com/track/youtube-dl-test-song",
        "https://a.bandcamp.com/track/x",
    ] {
        let canonical = classify_url(input).unwrap_or_else(|e| panic!("rejected {input}: {e}"));
        assert_eq!(canonical.as_str(), input);
    }
}

#[test]
fn rejects_unsupported_or_malformed_sources_before_host_use() {
    for input in [
        "",
        "http://a.bandcamp.com/track/x",
        "HTTPS://a.bandcamp.com/track/x",
        "https://a.bandcamp.com/track/x?q=1",
        "https://a.bandcamp.com/track/x#frag",
        "https://user@a.bandcamp.com/track/x",
        "https://a.bandcamp.com:443/track/x",
        "https://a.bandcamp.com/album/x",
        "https://a.bandcamp.com/track/",
        "https://a.bandcamp.com/track/x/extra",
        "https://www.bandcamp.com/track/x",
        "https://bandcamp.com/track/x",
        "https://a.Bandcamp.com/track/x",
        "https://A.bandcamp.com/track/x",
        "https://a.bandcamp.com/track/X",
        "https://a.bandcamp.com/track/x_y",
        "https://evil.bandcamp.com.example/track/x",
        "https://a.bandcamp.com/track/x/../y",
        "https://a-.bandcamp.com/track/x",
        "https://-a.bandcamp.com/track/x",
        "a.bandcamp.com/track/x",
        "https://a.bandcamp.com/track/",
    ] {
        assert!(classify_url(input).is_err(), "accepted {input}");
    }
}

#[test]
fn enforces_ascii_and_length_boundaries() {
    let long_artist = "a".repeat(64);
    let long_slug = "x".repeat(129);
    assert!(classify_url(&format!("https://{}.bandcamp.com/track/x", "a".repeat(63))).is_ok());
    assert!(classify_url(&format!("https://{long_artist}.bandcamp.com/track/x")).is_err());
    assert!(classify_url(&format!("https://a.bandcamp.com/track/{}", "x".repeat(128))).is_ok());
    assert!(classify_url(&format!("https://a.bandcamp.com/track/{long_slug}")).is_err());
    assert!(classify_url("https://a.bandcamp.com/track/ño").is_err());
    assert!(
        classify_url(&format!(
            "https://a.bandcamp.com/track/{}",
            "x".repeat(2_019)
        ))
        .is_err()
    );
}

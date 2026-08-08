use x::classify_url;

#[test]
fn accepts_the_canonical_shapes() {
    for (source, expected) in [
        (
            "https://x.com/LisPower1/status/1001551623938805763",
            "1001551623938805763",
        ),
        ("https://www.x.com/a/status/1", "1"),
        (
            "https://twitter.com/GunB1g/status/1163218564784017422",
            "1163218564784017422",
        ),
        ("https://www.twitter.com/a_b/status/12", "12"),
        ("https://mobile.twitter.com/a/status/12/", "12"),
    ] {
        assert_eq!(
            classify_url(source).map(|id| id.as_str().to_owned()),
            Ok(expected.to_owned()),
            "{source}"
        );
    }
}

/// Every share sheet appends tracking parameters, and a tap on one attachment
/// adds a trailing view segment. Both name the same post, and neither reaches
/// the network -- the request is rebuilt from the id.
#[test]
fn ignores_share_sheet_noise() {
    for source in [
        "https://x.com/a/status/1234567890123456789?s=20",
        "https://x.com/a/status/1234567890123456789?t=abc&s=20",
        "https://x.com/a/status/1234567890123456789#anchor",
        "https://x.com/a/status/1234567890123456789/photo/1",
        "https://x.com/a/status/1234567890123456789/video/2/",
    ] {
        assert_eq!(
            classify_url(source).map(|id| id.as_str().to_owned()),
            Ok("1234567890123456789".to_owned()),
            "{source}"
        );
    }
}

#[test]
fn refuses_everything_else() {
    for source in [
        "",
        "http://x.com/a/status/1",
        "https://x.com/a/status/",
        "https://x.com/a/status/abc",
        "https://x.com/a/status/01",
        "https://x.com/a/status/12345678901234567890",
        "https://x.com//status/1",
        "https://x.com/a/statuses/1",
        "https://x.com/a/status/1/extra",
        "https://x.com/a/status/1/photo",
        "https://x.com/a/status/1/photo/abc",
        "https://x.com/a",
        "https://x.com/i/spaces/1",
        "https://user@x.com/a/status/1",
        "https://x.com:8443/a/status/1",
        "https://notx.com/a/status/1",
        "https://x.com.evil.tld/a/status/1",
        "https://evil.x.com/a/status/1",
        "https://x.com/sixteencharacter/status/1",
        "https://x.com/has-hyphen/status/1",
    ] {
        assert!(classify_url(source).is_err(), "{source}");
    }
}

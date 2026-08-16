use bex_media_url_resolver_v2::{
    EphemeralFbDtsg, TahoeRequest, build_tahoe_request, validate_tahoe_request,
};

const TAHOE_URL: &str = "https://www.facebook.com/video/tahoe/async/10107927396957931/";

fn tahoe_request(url: &str, fb_dtsg: &str, pkg_cohort: &str, client_rev: &str) -> TahoeRequest {
    TahoeRequest {
        url: url.into(),
        fb_dtsg: fb_dtsg.into(),
        pkg_cohort: pkg_cohort.into(),
        client_rev: client_rev.into(),
    }
}

#[test]
fn validate_tahoe_request_accepts_valid() {
    let req = tahoe_request(TAHOE_URL, "valid_fb_dtsg", "PHASED:DEFAULT", "123456");
    assert!(validate_tahoe_request(&req).is_ok());
}

#[test]
fn validate_tahoe_request_rejects_empty_url() {
    let req = tahoe_request("", "valid_fb_dtsg", "PHASED:DEFAULT", "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_non_https_url() {
    let req = tahoe_request(
        "http://www.facebook.com/video/tahoe/async/10107927396957931/",
        "valid_fb_dtsg",
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_oversized_fb_dtsg() {
    let oversize = "x".repeat(256 + 1); // exceeds FB_DTSG bound of 256
    let req = tahoe_request(TAHOE_URL, &oversize, "PHASED:DEFAULT", "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_empty_fb_dtsg() {
    let req = tahoe_request(TAHOE_URL, "", "PHASED:DEFAULT", "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_non_graphic_fb_dtsg() {
    let req = tahoe_request(TAHOE_URL, "bad\0token", "PHASED:DEFAULT", "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_oversized_pkg_cohort() {
    let oversize = "x".repeat(128 + 1); // exceeds PKG_COHORT bound of 128
    let req = tahoe_request(TAHOE_URL, "valid_fb_dtsg", &oversize, "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_empty_pkg_cohort() {
    let req = tahoe_request(TAHOE_URL, "valid_fb_dtsg", "", "123456");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_oversized_client_rev() {
    let oversize = "1".repeat(64 + 1); // exceeds CLIENT_REV bound of 64
    let req = tahoe_request(TAHOE_URL, "valid_fb_dtsg", "PHASED:DEFAULT", &oversize);
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_non_digit_client_rev() {
    let req = tahoe_request(TAHOE_URL, "valid_fb_dtsg", "PHASED:DEFAULT", "12abc");
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_non_facebook_url() {
    let req = tahoe_request(
        "https://www.instagram.com/video/tahoe/async/10107927396957931/",
        "valid_fb_dtsg",
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(validate_tahoe_request(&req).is_err());
}

#[test]
fn validate_tahoe_request_rejects_url_with_userinfo_or_port() {
    let userinfo = tahoe_request(
        "https://user@www.facebook.com/video/tahoe/async/10107927396957931/",
        "valid_fb_dtsg",
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(validate_tahoe_request(&userinfo).is_err());

    let port = tahoe_request(
        "https://www.facebook.com:8080/video/tahoe/async/10107927396957931/",
        "valid_fb_dtsg",
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(validate_tahoe_request(&port).is_err());
}

#[test]
fn build_tahoe_request_validates_and_produces_call() {
    let call = build_tahoe_request(
        TAHOE_URL,
        EphemeralFbDtsg::new("valid_fb_dtsg".into()).unwrap(),
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(call.is_ok());
}

#[test]
fn build_tahoe_request_rejects_invalid() {
    let result = build_tahoe_request(
        "http://evil.com/",
        EphemeralFbDtsg::new("valid_fb_dtsg".into()).unwrap(),
        "PHASED:DEFAULT",
        "123456",
    );
    assert!(result.is_err());
}

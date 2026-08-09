// Spec Req 1 classifier scenarios — openspec/changes/add-tiktok-resolver/
// specs/tiktok-video-resolution/spec.md:15-93.
//
// RED state: `classify_url` is not yet exported by the tiktok crate, so this
// test file fails to compile. GREEN lands in the next commit when
// `src/url.rs` + `src/error.rs` + module wiring are added.

use tiktok::classify_url;

#[test]
fn accepts_canonical_user_video_url() {
    // spec.md:25-28 — Accepts canonical user/video URL
    let input = "https://www.tiktok.com/@pokemonlife22/video/7059698374567611694";
    let canonical = classify_url(input).expect("admitted");
    assert_eq!(canonical.as_str(), input);
}

#[test]
fn accepts_canonical_user_underscore_sentinel_url() {
    // spec.md:30-34 — `_` is a canonical user segment, NOT a sentinel
    let input = "https://www.tiktok.com/@_/video/7059698374567611694";
    let canonical = classify_url(input).expect("admitted");
    assert_eq!(canonical.as_str(), input);
}

#[test]
fn accepts_username_with_dots_and_underscores() {
    // spec.md:36-39 — `@`-segment contains `.`, `_`, `-` within [A-Za-z0-9._-]{1,64}
    let input = "https://www.tiktok.com/@user.name_v2/video/123";
    let canonical = classify_url(input).expect("admitted");
    assert_eq!(canonical.as_str(), input);
}

#[test]
fn rejects_query_string_canonical_url() {
    // spec.md:41-44 — query string before any host call
    assert!(classify_url("https://www.tiktok.com/@user/video/123?lang=en").is_err());
}

#[test]
fn rejects_fragment_canonical_url() {
    // spec.md:46-49 — fragment
    assert!(classify_url("https://www.tiktok.com/@user/video/123#t=1").is_err());
}

#[test]
fn rejects_vm_tiktok_com_short_link() {
    // spec.md:51-55 — vm.tiktok.com short link
    assert!(classify_url("https://vm.tiktok.com/Z123abc/").is_err());
}

#[test]
fn rejects_vt_tiktok_com_short_link() {
    // spec.md:57-60 — vt.tiktok.com short link
    assert!(classify_url("https://vt.tiktok.com/Z123abc/").is_err());
}

#[test]
fn rejects_www_tiktok_com_t_short_link() {
    // spec.md:62-65 — www.tiktok.com/t/<id> short link
    assert!(classify_url("https://www.tiktok.com/t/Z123abc/").is_err());
}

#[test]
fn rejects_profile_only_url() {
    // spec.md:67-71 — profile-only @user
    assert!(classify_url("https://www.tiktok.com/@user").is_err());
}

#[test]
fn rejects_live_url() {
    // spec.md:73-77 — @user/live
    assert!(classify_url("https://www.tiktok.com/@user/live").is_err());
}

#[test]
fn rejects_m_tiktok_com_share_live_host() {
    // spec.md:79-82 — m.tiktok.com is non-canonical
    assert!(classify_url("https://m.tiktok.com/share/live/12345").is_err());
}

#[test]
fn rejects_webcast_tiktok_com_host() {
    // spec.md:84-87 — webcast.tiktok.com
    assert!(classify_url("https://webcast.tiktok.com/anywhere").is_err());
}

#[test]
fn rejects_www_douyin_com_separate_site() {
    // spec.md:89-93 — Douyin is a separate site and change
    assert!(classify_url("https://www.douyin.com/video/12345").is_err());
}

// TRIANGULATE — adversarial near-miss host vectors per
// compatibility/v2/abi-identity-vectors.json deceptive-host / wildcard-host
// shapes and the new-extractor additive invariant (exact-match authority).

#[test]
fn rejects_deceptive_host_evil_tiktok_tld() {
    // deceptive-host: looks like tiktok but wrong TLD
    assert!(classify_url("https://evil-tiktok.tld/@user/video/123").is_err());
}

#[test]
fn rejects_subdomain_cheat_tiktok_attacker_com() {
    // subdomain cheat: tiktok as a label on an attacker domain
    assert!(classify_url("https://tiktok.attacker.com/@user/video/123").is_err());
}

#[test]
fn rejects_bare_apex_tiktok_com() {
    // bare apex without www: exact-match predicate requires www.tiktok.com
    assert!(classify_url("https://tiktok.com/@user/video/123").is_err());
}

#[test]
fn rejects_suffix_abuse_www_tiktok_com_evil_com() {
    // suffix abuse: www.tiktok.com embedded as a label prefix of an attacker host
    assert!(classify_url("https://www.tiktok.com.evil.com/@user/video/123").is_err());
}

#[test]
fn rejects_gateway_path_prefix_right_host_wrong_shape() {
    // www.tiktok.com/aweme/v1/play/ is the internal player redirect gateway.
    // Right host, but fails the /@{user}/video/{id} shape — rejected by the
    // path-bound check before any host call (proposal.md:165-168).
    assert!(classify_url("https://www.tiktok.com/aweme/v1/play/").is_err());
    assert!(classify_url("https://www.tiktok.com/aweme/v1/play/?item_id=123").is_err());
}
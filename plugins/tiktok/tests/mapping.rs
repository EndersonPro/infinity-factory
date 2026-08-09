// Spec Req 5 media URL resolution + Req 6 download-disabled video scenarios —
// openspec/changes/add-tiktok-resolver/specs/tiktok-video-resolution/spec.md:195-275.
//
// RED state: `map`, `is_safe_tiktok_output_url`, and `is_gateway_url` are not
// yet exported by the tiktok crate, so this file fails to compile. GREEN lands
// in the next commit when `src/mapping.rs` applies the CDN-family filter, the
// aweme/v1/play gateway rejection, dedup, and the 16-candidate cap.

use bex_media_url_resolver_v2::Resolution;
use std::fs;
use tiktok::{is_gateway_url, is_safe_tiktok_output_url, map, parse_universal_data, VideoData};
use url::Url;

const FIXTURE_DIR: &str = "tests/fixtures";

fn fixture(name: &str) -> Vec<u8> {
    fs::read(format!("{FIXTURE_DIR}/{name}")).expect("fixture present")
}

fn video(
    play_addr: Option<&str>,
    download_addr: Option<&str>,
    play_addr_struct_url_list: Vec<String>,
    bitrate_info_url_lists: Vec<String>,
) -> VideoData {
    VideoData {
        status_code: 0,
        play_addr: play_addr.map(str::to_owned),
        download_addr: download_addr.map(str::to_owned),
        play_addr_struct_url_list,
        bitrate_info_url_lists,
    }
}

fn parse_url(value: &str) -> Url {
    Url::parse(value).expect("valid url")
}

// Scenario: Returns Direct single candidate for one-URL video (spec.md:209-212)
#[test]
fn returns_direct_single_candidate_for_one_url_video() {
    let url = "https://v16-webapp-prime.tiktok.com/video/tos/alisg/only/?mime_type=video_mp4";
    let v = video(Some(url), None, vec![], vec![]);
    match map(&v) {
        Resolution::Direct(stream) => assert_eq!(stream.url, url),
        other => panic!("expected direct, got {other:?}"),
    }
}

// Scenario: Returns Candidates of CDN URLs for typical multi-URL video (spec.md:214-218)
#[test]
fn returns_candidates_of_cdn_urls_for_typical_multi_url_video() {
    let v = parse_universal_data(&fixture("tt_v3.html")).expect("probe parses");
    match map(&v) {
        Resolution::Candidates(items) => {
            assert!((2..=16).contains(&items.len()), "candidates within bound");
            for candidate in &items {
                let url = Url::parse(&candidate.stream.url).expect("candidate url parses");
                assert!(is_safe_tiktok_output_url(&url), "candidate on admitted family");
                assert!(!is_gateway_url(&url), "no gateway in candidates");
            }
        }
        other => panic!("expected candidates, got {other:?}"),
    }
}

// Scenario: Filters out www.tiktok.com/aweme/v1/play gateway URLs from candidates (spec.md:220-226)
#[test]
fn filters_out_gateway_urls_from_candidates() {
    let v = parse_universal_data(&fixture("tt_v3.html")).expect("probe parses");
    let items = match map(&v) {
        Resolution::Candidates(items) => items,
        other => panic!("expected candidates, got {other:?}"),
    };
    assert!(
        items
            .iter()
            .all(|c| !c.stream.url.starts_with("https://www.tiktok.com/aweme/v1/play/")),
        "gateway URL must not survive into candidates"
    );
    let hosts: Vec<&str> = items
        .iter()
        .filter_map(|c| Url::parse(&c.stream.url).ok().and_then(|u| u.host_str().map(Into::into)))
        .collect();
    assert!(hosts.iter().any(|h| h.starts_with("v16-webapp-prime")), "v16 CDN retained");
    assert!(hosts.iter().any(|h| h.starts_with("v19-webapp-prime")), "v19 CDN retained");
}

// Scenario: Deduplicates URLs across playAddr/downloadAddr/UrlList/bitrateInfo (spec.md:228-232)
#[test]
fn deduplicates_urls_across_sources() {
    let shared = "https://v16-webapp-prime.tiktok.com/video/tos/alisg/shared/?mime_type=video_mp4";
    let v = video(
        Some(shared),
        None,
        vec![shared.to_owned()],
        vec![shared.to_owned()],
    );
    match map(&v) {
        Resolution::Direct(stream) => assert_eq!(stream.url, shared),
        other => panic!("expected direct after dedupe, got {other:?}"),
    }
}

// Scenario: Returns Unsupported when no CDN URL survives filtering (spec.md:234-237)
#[test]
fn returns_unsupported_when_no_cdn_url_survives_filtering() {
    let gateway = "https://www.tiktok.com/aweme/v1/play/?item_id=123";
    let bare_apex = "https://tiktokcdn.com/video/a.mp4";
    let v = video(None, None, vec![gateway.to_owned(), bare_apex.to_owned()], vec![]);
    assert!(matches!(map(&v), Resolution::Unsupported(_)));
}

// Scenario: Returns Unsupported when the video has no PlayAddrStruct (slideshow) (spec.md:239-243)
#[test]
fn returns_unsupported_when_video_has_no_playaddrstruct_slideshow() {
    let v = video(None, None, vec![], vec![]);
    assert!(matches!(map(&v), Resolution::Unsupported(_)));
}

// Scenario: Truncates candidates to 16 per SDK bound (spec.md:245-248)
#[test]
fn truncates_candidates_to_sixteen_per_sdk_bound() {
    let urls: Vec<String> = (0..18)
        .map(|i| format!("https://v16-webapp-prime.tiktok.com/video/tos/alisg/{i}/?m=mp4"))
        .collect();
    let v = video(None, None, urls, vec![]);
    match map(&v) {
        Resolution::Candidates(items) => assert_eq!(items.len(), 16, "capped at 16"),
        other => panic!("expected candidates, got {other:?}"),
    }
}

// Scenario: Rejects candidate URLs whose host is not in admitted output family (spec.md:250-254)
#[test]
fn rejects_candidate_urls_whose_host_is_not_in_admitted_output_family() {
    let bare_apex = "https://tiktokcdn.com/video/a.mp4";
    let impersonator = "https://evil.tiktokcdn-impersonator.com/video/a.mp4";
    // Predicate-level rejection: both unsafe hosts fail the suffix-exact non-apex checks.
    assert!(!is_safe_tiktok_output_url(&parse_url(bare_apex)));
    assert!(!is_safe_tiktok_output_url(&parse_url(impersonator)));
    // Integration: only unsafe URLs present -> 0 survivors -> Unsupported.
    let v = video(
        Some(bare_apex),
        None,
        vec![impersonator.to_owned()],
        vec![],
    );
    assert!(matches!(map(&v), Resolution::Unsupported(_)));
}

// Scenario: Resolves via playAddr when downloadAddr absent (downloadSetting != 0) (spec.md:264-269)
#[test]
fn resolves_via_playaddr_when_downloadaddr_absent() {
    let play_addr = "https://v16-webapp-prime.tiktok.com/video/tos/alisg/playaddr/?mime_type=video_mp4";
    let v = video(Some(play_addr), None, vec![], vec![]);
    match map(&v) {
        Resolution::Direct(stream) => assert_eq!(stream.url, play_addr),
        other => panic!("expected direct from playAddr, got {other:?}"),
    }
}

// Scenario: Resolves via playAddr when both playAddr and downloadAddr present (spec.md:271-275)
#[test]
fn resolves_via_playaddr_when_both_playaddr_and_downloadaddr_present() {
    let play_addr = "https://v16-webapp-prime.tiktok.com/video/tos/alisg/play/?mime_type=video_mp4";
    let download_addr = "https://v19-webapp-prime.tiktok.com/video/tos/alisg/download/?mime_type=video_mp4";
    let v = video(Some(play_addr), Some(download_addr), vec![], vec![]);
    match map(&v) {
        Resolution::Candidates(items) => {
            let urls: Vec<&str> = items.iter().map(|c| c.stream.url.as_str()).collect();
            assert!(urls.contains(&play_addr), "playAddr included among candidates");
            assert!(urls.contains(&download_addr), "downloadAddr included among candidates");
        }
        other => panic!("expected candidates, got {other:?}"),
    }
}
// Spec Req 3 HTML JSON extraction + Req 4 StatusCode mapping scenarios —
// openspec/changes/add-tiktok-resolver/specs/tiktok-video-resolution/spec.md:129-193.
//
// RED state: `parse_universal_data` and `VideoData` are not yet exported by the
// tiktok crate, so this file fails to compile. GREEN lands in the next commit
// when `src/payload.rs` binds `__DEFAULT_SCOPE__.webapp.video-detail.itemInfo.
// itemStruct.video` and gates on statusCode.

use bex_media_url_resolver_v2::{Resolution, ResolverErrorKind};
use std::fs;
use tiktok::{parse_and_map, parse_universal_data};

const FIXTURE_DIR: &str = "tests/fixtures";

/// Wrap a raw universal-data JSON string in the exact TikTok `<script>` block
/// the resolver scans for, so synthetic `.json` fixtures parse as HTML bodies.
fn page(inner_json: &str) -> Vec<u8> {
    format!(
        "<html><head></head><body>\
         <script id=\"__UNIVERSAL_DATA_FOR_REHYDRATION__\" type=\"application/json\">\
         {inner_json}\
         </script></body></html>"
    )
    .into_bytes()
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(format!("{FIXTURE_DIR}/{name}")).expect("fixture present")
}

fn page_fixture(name: &str) -> Vec<u8> {
    page(std::str::from_utf8(&fixture(name)).expect("utf-8 fixture"))
}

fn mobile_page(inner_json: &str) -> Vec<u8> {
    format!(
        "<html><body><script id=\"api-data\" type=\"application/json\">\
         {inner_json}\
         </script></body></html>"
    )
    .into_bytes()
}

// Scenario: Parses universal data block to video struct (spec.md:140-144)
#[test]
fn parses_universal_data_block_to_video_struct() {
    let body = fixture("tt_v3.html");
    let video = parse_universal_data(&body).expect("happy path probe parses");
    assert_eq!(video.status_code, 0);
    let play_addr = video.play_addr.expect("playAddr populated");
    assert!(
        play_addr.starts_with("https://v16-webapp-prime.tiktok.com/"),
        "playAddr is a safe CDN url: {play_addr}"
    );
    assert!(video.download_addr.is_some());
    // PlayAddrStruct.UrlList carries 2 CDN entries + 1 www.tiktok.com gateway.
    assert_eq!(video.play_addr_struct_url_list.len(), 3);
    assert_eq!(video.bitrate_info_url_lists.len(), 3);
}

/// TikTok currently serves public detail pages to the host's anonymous mobile
/// browser identity with `api-data`, while the older desktop rendering used
/// universal rehydration. Both are static JSON script islands carrying the
/// identical video-detail contract.
#[test]
fn parses_mobile_api_data_video_detail() {
    let body = mobile_page(
        r#"{"videoDetail":{"statusCode":0,"itemInfo":{"itemStruct":{"video":{
            "playAddr":"https://v16-webapp-prime.tiktok.com/video/tos/alisg/mobile/?mime_type=video_mp4"
        }}}}}"#,
    );
    let video = parse_universal_data(&body).expect("mobile api-data parses");
    assert_eq!(video.status_code, 0);
    assert_eq!(
        video.play_addr.as_deref(),
        Some("https://v16-webapp-prime.tiktok.com/video/tos/alisg/mobile/?mime_type=video_mp4")
    );
    let resolved = parse_and_map(&body).expect("mobile api-data maps");
    assert!(matches!(resolved.resolution, Resolution::Direct(_)));
}

// Scenario: Returns Unsupported when universal data block is absent (spec.md:146-150)
#[test]
fn returns_unsupported_when_universal_data_block_is_absent() {
    let body = fixture("synthetic_no_universal_block.html");
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

// Scenario: Returns Unsupported when itemInfo.itemStruct.video is absent (spec.md:152-156)
#[test]
fn returns_unsupported_when_iteminfo_itemstruct_video_is_absent() {
    let body = page_fixture("synthetic_empty_video.json");
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

// Scenario: Returns Unsupported when JSON is malformed (spec.md:158-161)
#[test]
fn returns_unsupported_when_json_is_malformed() {
    let body = page_fixture("synthetic_malformed.json");
    let error = parse_universal_data(&body).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::UnsupportedUrl);
    // No leaked upstream content surfaces in the typed error.
    assert!(!format!("{error:?}").contains("ParseError expected"));
}

// Scenario: statusCode 10204 status_self_see maps to Unsupported (spec.md:172-176)
#[test]
fn status_code_10204_status_self_see_maps_to_unsupported() {
    let body = page_fixture("synthetic_10204_status_self_see.json");
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

// Scenario: statusCode 10204 person_geo_fencing maps to Unsupported (spec.md:178-182)
#[test]
fn status_code_10204_person_geo_fencing_maps_to_unsupported() {
    let body = page_fixture("synthetic_10204_geo_fencing.json");
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

// Scenario: statusCode 10204 item doesn't exist maps to Unsupported (spec.md:184-188)
#[test]
fn status_code_10204_item_doesnt_exist_maps_to_unsupported() {
    let body = page_fixture("synthetic_10204_item_doesnt_exist.json");
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

// Scenario: statusCode not 0 and not 10204 maps to Unsupported (defensive) (spec.md:190-193)
#[test]
fn status_code_not_zero_not_10204_maps_to_unsupported_defensively() {
    let body = page(
        r#"{"__DEFAULT_SCOPE__":{"webapp.video-detail":{
            "statusCode": 9999,
            "itemInfo":{"itemStruct":{"video":{
                "playAddr":"https://v16-webapp-prime.tiktok.com/x/?mime_type=video_mp4"
            }}}
        }}}"#,
    );
    assert_eq!(
        parse_universal_data(&body).unwrap_err().kind,
        ResolverErrorKind::UnsupportedUrl
    );
}

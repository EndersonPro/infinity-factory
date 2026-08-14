use bex_media_url_resolver_v2::{ResolverErrorKind, Resolution};
use facebook::{
    Media, extract_data_sjs, extract_media, extract_tahoe_tokens, is_login_wall,
    map_progressive, parse_tahoe_response, unsupported_response,
};

const FB_VIDEO: &str = include_str!("fixtures/fb_video.html");
const FB_TAHOE_PAGE: &str = include_str!("fixtures/fb_tahoe_page.html");
const FB_LOGIN_WALL: &str = include_str!("fixtures/fb_login_wall.html");
const FB_TAHOE_JSON: &[u8] = include_bytes!("fixtures/fb_tahoe.json");

fn sample_media(progressive: Vec<&str>) -> Media {
    Media {
        progressive: progressive.into_iter().map(String::from).collect(),
        has_dash_hls: false,
        title: Some("Title".into()),
        author: Some("Author".into()),
        thumbnail: Some("https://scontent.example.invalid/thumb.jpg".into()),
        duration_milliseconds: Some(1000),
    }
}

#[test]
fn extracts_progressive_sd_hd_and_metadata_from_data_sjs() {
    let blocks = extract_data_sjs(FB_VIDEO);
    assert!(!blocks.is_empty(), "a data-sjs block is located");
    let media = extract_media(&blocks)
        .expect("no transport error")
        .expect("a video media object is selected");
    assert_eq!(
        media.progressive,
        [
            "https://video.example.invalid/sd.mp4",
            "https://video.example.invalid/hd.mp4",
        ]
    );
    assert!(!media.has_dash_hls);
    assert_eq!(media.title.as_deref(), Some("Public video"));
    assert_eq!(media.author.as_deref(), Some("Zuck"));
    assert_eq!(
        media.thumbnail.as_deref(),
        Some("https://scontent.example.invalid/thumb.jpg")
    );
    assert_eq!(media.duration_milliseconds, Some(90_000));
}

#[test]
fn data_sjs_without_progressive_means_tahoe_fallback() {
    assert!(!is_login_wall(FB_TAHOE_PAGE));
    let blocks = extract_data_sjs(FB_TAHOE_PAGE);
    let media = extract_media(&blocks)
        .expect("no transport error")
        .expect("a video media object is present");
    assert!(media.progressive.is_empty(), "no progressive URL survives");
    assert!(
        !media.has_dash_hls,
        "not DASH/HLS-only, so Tahoe fallback applies (not Unsupported)"
    );
}

#[test]
fn login_wall_markers_detected_and_take_precedence_at_parser_layer() {
    assert!(is_login_wall(FB_LOGIN_WALL));
    assert!(!is_login_wall(FB_VIDEO));
    // The login page still carries a partial data-sjs block, but the markers
    // win first; its block parses to no video media.
    let blocks = extract_data_sjs(FB_LOGIN_WALL);
    assert!(extract_media(&blocks).expect("parses").is_none());
}

#[test]
fn parses_tahoe_response_after_stripping_prefix() {
    let urls = parse_tahoe_response(FB_TAHOE_JSON).expect("tahoe sd/hd sources");
    assert_eq!(
        urls,
        [
            "https://video.example.invalid/tahoe-sd.mp4",
            "https://video.example.invalid/tahoe-hd.mp4",
        ]
    );
    // After stripping the sentinel, malformed JSON maps to malformed-response.
    assert_eq!(
        parse_tahoe_response(b"for (;;);not-json")
            .unwrap_err()
            .kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn dash_or_hls_only_has_no_progressive_but_sets_dash_flag() {
    let dash_only = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[["VideoConfig",[],{"video_data":{"video_id":"1","playable_url_dash":"https://video.example.invalid/dash.mpd"}}]]]]}</script>"#;
    let media = extract_media(&extract_data_sjs(dash_only))
        .expect("parses")
        .expect("media present");
    assert!(media.progressive.is_empty());
    assert!(media.has_dash_hls);

    let mixed = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[["VideoConfig",[],{"video_data":{"video_id":"1","playable_url":"https://video.example.invalid/sd.mp4","playable_url_dash":"https://video.example.invalid/dash.mpd"}}]]]]}</script>"#;
    let media = extract_media(&extract_data_sjs(mixed))
        .expect("parses")
        .expect("media present");
    assert_eq!(media.progressive, ["https://video.example.invalid/sd.mp4"]);
    assert!(media.has_dash_hls, "the DASH field is still observed");
}

#[test]
fn reels_resolve_via_short_form_video_context() {
    let reel = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"creation_story":{"short_form_video_context":{"playback_video":{"video_id":"1","playable_url":"https://video.example.invalid/reel.mp4"}}}}</script>"#;
    let media = extract_media(&extract_data_sjs(reel))
        .expect("parses")
        .expect("media present");
    assert_eq!(media.progressive, ["https://video.example.invalid/reel.mp4"]);
}

#[test]
fn malformed_data_sjs_block_maps_to_malformed_response() {
    let bad = r#"<script type="application/json" data-sjs="ScheduledServerJS">not-json</script>"#;
    let blocks = extract_data_sjs(bad);
    assert_eq!(
        extract_media(&blocks).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn absent_data_sjs_or_absent_media_yield_no_media() {
    assert!(extract_data_sjs("<html></html>").is_empty());
    assert!(extract_media(&extract_data_sjs("<html></html>"))
        .expect("parses")
        .is_none());
    let no_media = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[]]]}</script>"#;
    assert!(extract_media(&extract_data_sjs(no_media))
        .expect("parses")
        .is_none());
}

#[test]
fn dedups_progressive_urls_against_top_level_fields() {
    let html = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[["VideoConfig",[],{"video_data":{"video_id":"1","playable_url":"https://video.example.invalid/sd.mp4","progressive_urls":[{"progressive_url":"https://video.example.invalid/sd.mp4","metadata":{"quality":"sd"}},{"progressive_url":"https://video.example.invalid/hd.mp4","metadata":{"quality":"hd"}}]}}]]]]}</script>"#;
    let media = extract_media(&extract_data_sjs(html))
        .expect("parses")
        .expect("media present");
    assert_eq!(
        media.progressive,
        [
            "https://video.example.invalid/sd.mp4",
            "https://video.example.invalid/hd.mp4",
        ]
    );
}

#[test]
fn rejects_http_and_userinfo_progressive_urls() {
    let html = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[["VideoConfig",[],{"video_data":{"video_id":"1","playable_url":"http://video.example.invalid/sd.mp4","browser_native_hd_url":"https://user:pass@video.example.invalid/hd.mp4"}}]]]]}</script>"#;
    let media = extract_media(&extract_data_sjs(html))
        .expect("parses")
        .expect("media present");
    assert!(media.progressive.is_empty(), "unsafe URLs are rejected");
}

#[test]
fn rejects_non_https_thumbnail_but_keeps_progressive() {
    let html = r#"<script type="application/json" data-sjs="ScheduledServerJS">{"require":[["ScheduledServerJS","handle",[["VideoConfig",[],{"video_data":{"video_id":"1","playable_url":"https://video.example.invalid/sd.mp4","thumbnail":{"uri":"http://scontent.example.invalid/thumb.jpg"}}}]]]]}</script>"#;
    let media = extract_media(&extract_data_sjs(html))
        .expect("parses")
        .expect("media present");
    assert_eq!(media.progressive, ["https://video.example.invalid/sd.mp4"]);
    assert!(media.thumbnail.is_none(), "non-https thumbnail is omitted");
}

#[test]
fn extracts_tahoe_tokens_when_present_absent_otherwise() {
    assert!(extract_tahoe_tokens(FB_TAHOE_PAGE).is_some());
    assert!(extract_tahoe_tokens(FB_VIDEO).is_none());
    assert!(extract_tahoe_tokens("<html></html>").is_none());
}

#[test]
fn maps_one_url_to_direct_and_many_to_candidates_with_metadata() {
    let media = sample_media(vec!["https://video.example.invalid/only.mp4"]);
    let response = map_progressive(&media);
    let Resolution::Direct(stream) = response.resolution else {
        panic!("expected direct for a single URL");
    };
    assert_eq!(stream.url, "https://video.example.invalid/only.mp4");
    assert_eq!(stream.mime_type.as_deref(), Some("video/mp4"));
    let metadata = response.metadata.expect("metadata emitted");
    assert_eq!(metadata.title.as_deref(), Some("Title"));
    assert_eq!(metadata.author.as_deref(), Some("Author"));
    assert_eq!(
        metadata.thumbnail_url.as_deref(),
        Some("https://scontent.example.invalid/thumb.jpg")
    );
    assert_eq!(metadata.duration_milliseconds, Some(1000));

    let media = sample_media(vec![
        "https://video.example.invalid/a.mp4",
        "https://video.example.invalid/b.mp4",
    ]);
    let response = map_progressive(&media);
    let Resolution::Candidates(items) = response.resolution else {
        panic!("expected candidates for distinct URLs");
    };
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].stream.url, "https://video.example.invalid/a.mp4");
    assert_eq!(items[1].stream.url, "https://video.example.invalid/b.mp4");
}

#[test]
fn mapping_omits_metadata_for_unsupported() {
    let response = unsupported_response("dash-only media");
    assert!(matches!(response.resolution, Resolution::Unsupported(_)));
    assert!(response.metadata.is_none());
}
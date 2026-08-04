use bex_media_url_resolver_v2::{MuxContainer, Resolution, ResolverErrorKind};
use youtube::parse_and_map;

const ID: &str = "dQw4w9WgXcQ";
const CIPHER_JS: &str = include_str!("../fixtures/player-legacy-cipher.js");

fn watch_html(player_response_json: &str, js_url: Option<&str>) -> Vec<u8> {
    let js_tag = match js_url {
        Some(url) => format!("<script>var ytplayer={{\"jsUrl\":\"{url}\"}};</script>"),
        None => String::new(),
    };
    format!(
        "<html><head></head><body><script nonce=\"x\">var ytInitialPlayerResponse = {player_response_json};var ytInitialData = {{}};</script>{js_tag}</body></html>"
    )
    .into_bytes()
}

const PROGRESSIVE_ONE: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"Test Video","author":"Test Channel","lengthSeconds":"212","thumbnail":{"thumbnails":[{"url":"https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg","width":480,"height":360}]}},"streamingData":{"formats":[{"itag":18,"mimeType":"video/mp4; codecs=\"avc1.42001E, mp4a.40.2\"","bitrate":500000,"height":360,"qualityLabel":"360p","url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=1&expire=9999999999"}]}}"#;

const PROGRESSIVE_TWO: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T","author":"A","lengthSeconds":"1"},"streamingData":{"formats":[{"itag":18,"mimeType":"video/mp4; codecs=\"avc1.42001E, mp4a.40.2\"","bitrate":500000,"height":360,"qualityLabel":"360p","url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=1&expire=1"},{"itag":22,"mimeType":"video/mp4; codecs=\"avc1.640028, mp4a.40.2\"","bitrate":2000000,"height":720,"qualityLabel":"720p","url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=2&expire=1"}]}}"#;

const ADAPTIVE_DIRECT_MP4: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T","author":"A","lengthSeconds":"1"},"streamingData":{"adaptiveFormats":[{"itag":140,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","bitrate":128000,"url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=2&expire=9999999999"},{"itag":137,"mimeType":"video/mp4; codecs=\"avc1.640028\"","bitrate":2500000,"height":1080,"qualityLabel":"1080p","url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=3&expire=9999999999"}]}}"#;

const ADAPTIVE_CIPHER_MIXED: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T","author":"A","lengthSeconds":"1"},"streamingData":{"adaptiveFormats":[{"itag":140,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","bitrate":128000,"url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=2&expire=9999999999"},{"itag":137,"mimeType":"video/webm; codecs=\"vp9\"","bitrate":2500000,"height":1080,"qualityLabel":"1080p","signatureCipher":"s=abcdefghij&sp=sig&url=https%3A%2F%2Frr1---sn-abc.googlevideo.com%2Fvideoplayback%3Fid%3D3%26expire%3D9999999999"}]}}"#;

const AUDIO_ONLY: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T","author":"A","lengthSeconds":"1"},"streamingData":{"adaptiveFormats":[{"itag":140,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","bitrate":128000,"url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=2&expire=1"}]}}"#;

const LOGIN_REQUIRED: &str = r#"{"playabilityStatus":{"status":"LOGIN_REQUIRED"}}"#;

const NO_FORMATS: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T"}}"#;

#[test]
fn maps_single_progressive_format_as_direct_with_metadata() {
    let response =
        parse_and_map(&watch_html(PROGRESSIVE_ONE, None), ID, None).expect("must resolve");
    let meta = response.metadata.unwrap();
    assert_eq!(meta.title.as_deref(), Some("Test Video"));
    assert_eq!(meta.author.as_deref(), Some("Test Channel"));
    assert_eq!(
        meta.thumbnail_url.as_deref(),
        Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg")
    );
    assert_eq!(meta.duration_milliseconds, Some(212_000));
    match response.resolution {
        Resolution::Direct(stream) => {
            assert_eq!(
                stream.url,
                "https://rr1---sn-abc.googlevideo.com/videoplayback?id=1&expire=9999999999"
            );
            assert_eq!(stream.format.as_deref(), Some("mp4"));
            assert_eq!(stream.mime_type.as_deref(), Some("video/mp4"));
            assert_eq!(stream.quality_label.as_deref(), Some("360p"));
            assert_eq!(stream.expires_at_unix_seconds, Some(9_999_999_999));
            assert!(stream.byte_range_supported);
        }
        other => panic!("expected direct, got {other:?}"),
    }
}

#[test]
fn maps_unsupported_when_no_formats_resolve() {
    let response = parse_and_map(&watch_html(NO_FORMATS, None), ID, None).unwrap();
    assert_eq!(
        response.metadata.unwrap().thumbnail_url.as_deref(),
        Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault.jpg")
    );
    match response.resolution {
        Resolution::Unsupported(_) => {}
        other => panic!("expected unsupported, got {other:?}"),
    }
}

#[test]
fn maps_multiple_progressive_formats_as_ordered_candidates() {
    let response = parse_and_map(&watch_html(PROGRESSIVE_TWO, None), ID, None).unwrap();
    match response.resolution {
        Resolution::Candidates(items) => {
            assert_eq!(items.len(), 2);
            // Sorted by height descending: 720p first.
            assert_eq!(items[0].stream.quality_label.as_deref(), Some("720p"));
            assert_eq!(items[1].stream.quality_label.as_deref(), Some("360p"));
        }
        other => panic!("expected candidates, got {other:?}"),
    }
}

#[test]
fn maps_direct_adaptive_pair_as_separated_mp4() {
    let response = parse_and_map(&watch_html(ADAPTIVE_DIRECT_MP4, None), ID, None).unwrap();
    match response.resolution {
        Resolution::Separated(pair) => {
            assert!(
                pair.audio
                    .mime_type
                    .as_deref()
                    .unwrap()
                    .starts_with("audio/")
            );
            assert!(
                pair.video
                    .mime_type
                    .as_deref()
                    .unwrap()
                    .starts_with("video/")
            );
            assert_eq!(pair.mux_plan.container, MuxContainer::Mp4);
            assert!(pair.mux_plan.prefer_stream_copy);
        }
        other => panic!("expected separated, got {other:?}"),
    }
}

#[test]
fn decodes_signature_cipher_and_maps_mixed_containers_as_matroska() {
    let js_url = "https://www.youtube.com/s/player/aaaaaaaa/player_ias.vflset/en_US/base.js";
    let response = parse_and_map(
        &watch_html(ADAPTIVE_CIPHER_MIXED, Some(js_url)),
        ID,
        Some(CIPHER_JS.as_bytes()),
    )
    .unwrap();
    match response.resolution {
        Resolution::Separated(pair) => {
            assert_eq!(
                pair.video.url,
                "https://rr1---sn-abc.googlevideo.com/videoplayback?id=3&expire=9999999999&sig=cgfedhba&ratebypass=yes"
            );
            assert_eq!(pair.mux_plan.container, MuxContainer::Matroska);
        }
        other => panic!("expected separated, got {other:?}"),
    }
}

#[test]
fn errors_when_cipher_is_needed_but_no_player_js_body_was_supplied() {
    let error = parse_and_map(&watch_html(ADAPTIVE_CIPHER_MIXED, None), ID, None).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::MalformedResponse);
}

#[test]
fn maps_audio_only_adaptive_as_direct() {
    let response = parse_and_map(&watch_html(AUDIO_ONLY, None), ID, None).unwrap();
    match response.resolution {
        Resolution::Direct(stream) => {
            assert!(stream.mime_type.as_deref().unwrap().starts_with("audio/"))
        }
        other => panic!("expected direct audio, got {other:?}"),
    }
}

#[test]
fn maps_login_required_status_as_private() {
    let error = parse_and_map(&watch_html(LOGIN_REQUIRED, None), ID, None).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
}

#[test]
fn rejects_page_missing_the_player_response_marker_as_malformed() {
    let error = parse_and_map(b"<html><body>no player data</body></html>", ID, None).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::MalformedResponse);
}

#[test]
fn resolves_from_the_first_of_duplicate_player_response_markers() {
    // Real watch pages routinely embed the marker more than once (ad/cue-point
    // config blocks re-declaring related globals); the first occurrence is
    // the real player state and must still resolve, not be rejected outright.
    let html = watch_html(PROGRESSIVE_ONE, None);
    let mut doubled = html.clone();
    doubled.extend_from_slice(&html);
    let response = parse_and_map(&doubled, ID, None).unwrap();
    assert!(matches!(response.resolution, Resolution::Direct(_)));
}

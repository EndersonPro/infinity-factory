use bex_media_url_resolver_v2::{
    ExpectedCall, GetRequest, HttpsError, HttpsResponse, MockHttpsClient, Resolution,
    ResolverErrorKind,
};
use youtube::resolve_public;

const ID: &str = "dQw4w9WgXcQ";
const WATCH_URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
const JS_URL: &str = "https://www.youtube.com/s/player/aaaaaaaa/player_ias.vflset/en_US/base.js";
const CIPHER_JS: &str = include_str!("../fixtures/player-legacy-cipher.js");

const PROGRESSIVE_ONE: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"Test Video","author":"Test Channel","lengthSeconds":"1"},"streamingData":{"formats":[{"itag":18,"mimeType":"video/mp4; codecs=\"avc1.42001E, mp4a.40.2\"","bitrate":500000,"height":360,"qualityLabel":"360p","url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=1&expire=1"}]}}"#;

const ADAPTIVE_CIPHER_MIXED: &str = r#"{"playabilityStatus":{"status":"OK"},"videoDetails":{"title":"T","author":"A","lengthSeconds":"1"},"streamingData":{"adaptiveFormats":[{"itag":140,"mimeType":"audio/mp4; codecs=\"mp4a.40.2\"","bitrate":128000,"url":"https://rr1---sn-abc.googlevideo.com/videoplayback?id=2&expire=9999999999"},{"itag":137,"mimeType":"video/webm; codecs=\"vp9\"","bitrate":2500000,"height":1080,"qualityLabel":"1080p","signatureCipher":"s=abcdefghij&sp=sig&url=https%3A%2F%2Frr1---sn-abc.googlevideo.com%2Fvideoplayback%3Fid%3D3%26expire%3D9999999999"}]}}"#;

const LOGIN_REQUIRED: &str = r#"{"playabilityStatus":{"status":"LOGIN_REQUIRED"}}"#;

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

fn resp(final_url: &str, status: u16, body: &[u8]) -> HttpsResponse {
    HttpsResponse {
        status,
        final_url: final_url.into(),
        headers: vec![],
        body: body.to_vec(),
    }
}

fn get(url: &str, result: Result<HttpsResponse, HttpsError>) -> ExpectedCall {
    ExpectedCall::Get(
        GetRequest {
            url: url.into(),
            headers: vec![],
        },
        result,
    )
}

#[test]
fn resolves_progressive_video_with_exactly_one_headerless_get() {
    let mut client = MockHttpsClient::new(vec![get(
        WATCH_URL,
        Ok(resp(WATCH_URL, 200, &watch_html(PROGRESSIVE_ONE, None))),
    )]);
    let response = resolve_public(&mut client, WATCH_URL).unwrap();
    assert_eq!(
        response.metadata.unwrap().title.as_deref(),
        Some("Test Video")
    );
    assert!(matches!(response.resolution, Resolution::Direct(_)));
    assert_eq!(
        client
            .observations()
            .iter()
            .map(|o| o.operation)
            .collect::<Vec<_>>(),
        ["get"]
    );
    assert!(client.verify().is_ok());
}

#[test]
fn resolves_ciphered_adaptive_video_with_watch_then_player_js_gets() {
    let mut client = MockHttpsClient::new(vec![
        get(
            WATCH_URL,
            Ok(resp(
                WATCH_URL,
                200,
                &watch_html(ADAPTIVE_CIPHER_MIXED, Some(JS_URL)),
            )),
        ),
        get(JS_URL, Ok(resp(JS_URL, 200, CIPHER_JS.as_bytes()))),
    ]);
    let response = resolve_public(&mut client, WATCH_URL).unwrap();
    match response.resolution {
        Resolution::Separated(pair) => {
            assert!(pair.video.url.contains("sig=cgfedhba"));
        }
        other => panic!("expected separated, got {other:?}"),
    }
    assert_eq!(
        client
            .observations()
            .iter()
            .map(|o| o.operation)
            .collect::<Vec<_>>(),
        ["get", "get"]
    );
    assert!(client.verify().is_ok());
}

#[test]
fn accepts_youtu_be_and_shorts_inputs_but_always_fetches_the_canonical_watch_url() {
    for input in [
        format!("https://youtu.be/{ID}"),
        format!("https://www.youtube.com/shorts/{ID}"),
        format!("https://m.youtube.com/watch?v={ID}"),
    ] {
        let mut client = MockHttpsClient::new(vec![get(
            WATCH_URL,
            Ok(resp(WATCH_URL, 200, &watch_html(PROGRESSIVE_ONE, None))),
        )]);
        resolve_public(&mut client, &input).unwrap_or_else(|e| panic!("{input}: {e:?}"));
        assert!(client.verify().is_ok(), "input: {input}");
    }
}

#[test]
fn rejects_invalid_source_with_zero_host_calls() {
    let mut client = MockHttpsClient::new(vec![get(
        WATCH_URL,
        Err(HttpsError::TransportFailure),
    )]);
    let error = resolve_public(&mut client, "https://evil.example/watch?v=x").unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::InvalidInput);
    assert!(client.observations().is_empty());
    assert!(client.verify().is_err());
}

#[test]
fn maps_login_required_playability_status_to_private_with_a_single_get() {
    let mut client = MockHttpsClient::new(vec![get(
        WATCH_URL,
        Ok(resp(WATCH_URL, 200, &watch_html(LOGIN_REQUIRED, None))),
    )]);
    let error = resolve_public(&mut client, WATCH_URL).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::PrivateOrUnavailable);
    assert_eq!(client.observations().len(), 1);
    assert!(client.verify().is_ok());
}

#[test]
fn maps_transport_and_status_failures_without_leaking_upstream_body() {
    for (result, kind, retryable) in [
        (Err(HttpsError::Timeout), ResolverErrorKind::Timeout, true),
        (
            Err(HttpsError::TransportFailure),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
        (
            Ok(resp(WATCH_URL, 401, b"SENSITIVE_BODY")),
            ResolverErrorKind::PrivateOrUnavailable,
            false,
        ),
        (
            Ok(resp(WATCH_URL, 404, b"SENSITIVE_BODY")),
            ResolverErrorKind::Unavailable,
            false,
        ),
        (
            Ok(resp(WATCH_URL, 429, b"SENSITIVE_BODY")),
            ResolverErrorKind::RateLimited,
            true,
        ),
        (
            Ok(resp(WATCH_URL, 500, b"SENSITIVE_BODY")),
            ResolverErrorKind::UpstreamFailure,
            true,
        ),
    ] {
        let mut client = MockHttpsClient::new(vec![get(WATCH_URL, result)]);
        let error = resolve_public(&mut client, WATCH_URL).unwrap_err();
        assert_eq!((error.kind, error.retryable), (kind, retryable));
        assert!(!format!("{error:?}").contains("SENSITIVE_BODY"));
        assert!(client.verify().is_ok());
    }
}

#[test]
fn rejects_off_origin_final_response_as_malformed() {
    let mut client = MockHttpsClient::new(vec![get(
        WATCH_URL,
        Ok(resp(
            "https://evil.example/watch?v=dQw4w9WgXcQ",
            200,
            &watch_html(PROGRESSIVE_ONE, None),
        )),
    )]);
    assert_eq!(
        resolve_public(&mut client, WATCH_URL).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn fails_closed_when_cipher_is_needed_but_no_player_js_url_is_found_on_the_page() {
    let mut client = MockHttpsClient::new(vec![get(
        WATCH_URL,
        Ok(resp(
            WATCH_URL,
            200,
            &watch_html(ADAPTIVE_CIPHER_MIXED, None),
        )),
    )]);
    let error = resolve_public(&mut client, WATCH_URL).unwrap_err();
    assert_eq!(error.kind, ResolverErrorKind::MalformedResponse);
    // Zero player-JS GET is issued: only the watch page GET is observed.
    assert_eq!(client.observations().len(), 1);
}

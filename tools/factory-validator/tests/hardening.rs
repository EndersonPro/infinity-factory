use factory_validator::{bounds, validate_contract, validate_fixture};
use serde_json::{Value, json};

const FIXTURE: &str = include_str!("../../../fixtures/resolver-responses/abi-v1.json");
const WIT: &str = include_str!("../../../wit/media-url-resolver/wit/media-url-resolver.wit");

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).expect("test fixture must be valid")
}
fn rejected(value: &Value) {
    assert!(validate_fixture(&value.to_string()).is_err());
}

#[test]
fn rejects_every_missing_resolution_variant() {
    for variant in [
        "direct",
        "candidates",
        "separated",
        "unsupported",
        "deferred",
    ] {
        assert!(
            validate_contract(&WIT.replacen(&format!("{variant}("), "removed(", 1)).is_err(),
            "variant remained unvalidated: {variant}"
        );
    }
}

#[test]
fn enforces_field_specific_string_bounds() {
    for (pointer, limit) in [
        ("/request/source_url", bounds::URL_BYTES),
        ("/request/correlation_id", bounds::CORRELATION_ID_BYTES),
        ("/metadata/title", bounds::TITLE_BYTES),
        ("/metadata/author", bounds::AUTHOR_BYTES),
        ("/metadata/thumbnail_url", bounds::URL_BYTES),
        (
            "/request/quality_preferences/0/container",
            bounds::FORMAT_BYTES,
        ),
        (
            "/request/quality_preferences/0/mime_type",
            bounds::MIME_AND_CODECS_BYTES,
        ),
        ("/request/client_context/0/key", bounds::CONTEXT_KEY_BYTES),
        (
            "/request/client_context/0/value",
            bounds::CONTEXT_VALUE_BYTES,
        ),
        ("/headers/0/name", bounds::HEADER_NAME_BYTES),
        ("/headers/0/value", bounds::HEADER_VALUE_BYTES),
        ("/responses/candidates/0/id", bounds::CANDIDATE_ID_BYTES),
        ("/responses/direct/stream/format", bounds::FORMAT_BYTES),
        (
            "/responses/direct/stream/codecs",
            bounds::MIME_AND_CODECS_BYTES,
        ),
        ("/responses/unsupported/reason", bounds::REASON_BYTES),
        ("/responses/deferred/reason", bounds::REASON_BYTES),
    ] {
        let mut value = fixture();
        *value
            .pointer_mut(pointer)
            .expect("test fixture must be valid") = "x".repeat(limit + 1).into();
        rejected(&value);
    }
}

#[test]
fn rejects_collection_duplicates_and_limits() {
    let mut value = fixture();
    value["request"]["client_context"] = json!([{"key":"a"},{"key":"a"}]);
    rejected(&value);
    let mut value = fixture();
    value["headers"] = json!([{"name":"Accept","value":"a"},{"name":"accept","value":"b"}]);
    rejected(&value);
    let mut value = fixture();
    value["responses"]["candidates"] =
        Value::Array(vec![json!({"id":"a"}); bounds::CANDIDATES + 1]);
    rejected(&value);
    let mut value = fixture();
    value["request"]["quality_preferences"] =
        Value::Array(vec![json!({}); bounds::QUALITY_PREFERENCES + 1]);
    rejected(&value);
    let mut value = fixture();
    value["responses"]["deferred"]["retry_after_seconds"] =
        (bounds::RETRY_AFTER_SECONDS + 1).into();
    rejected(&value);
    let mut value = fixture();
    value["error_samples"][0]["safe_message"] =
        "x".repeat(bounds::SAFE_ERROR_MESSAGE_BYTES + 1).into();
    rejected(&value);
}

#[test]
fn rejects_every_insecure_returned_url_and_prohibited_token() {
    for pointer in [
        "/request/source_url",
        "/responses/direct/stream/url",
        "/responses/candidates/0/stream/url",
        "/responses/separated/audio/url",
        "/responses/separated/video/url",
        "/metadata/thumbnail_url",
    ] {
        let mut value = fixture();
        *value
            .pointer_mut(pointer)
            .expect("test fixture must be valid") = "http://unsafe.test/media".into();
        rejected(&value);
    }
    for token in [
        "WebView",
        "JavaScript",
        "HMAC",
        "ffmpeg",
        "main.api.progmore.com",
    ] {
        assert!(validate_fixture(&FIXTURE.replace("Fixture", token)).is_err());
    }
}

//! Standalone behavior suite for the BEX test-host fixture engine.

use std::clone::Clone;
use std::cmp::{Eq, PartialEq};
use std::fmt::Debug;

use bex_test_host::contracts::HostError;
use bex_test_host::fixture::{
    FailureCoordinate, Field, FieldCategory, FixtureEngine, FixtureExchange, FixtureFailure,
    FixtureFailureKind, FixtureOutcome, FixtureReply, FixtureReplyError, FixtureRequest,
    FixtureResponse, Observation, ObservedField, Operation, sanitize_name, sanitize_url,
};

const SENTINEL: &str = "SECRET-s3ntinel-VALue!";
const POLICY: &str = "media-url-resolver";
const URL: &str = "https://example.com/";

fn field(name: &str, category: FieldCategory, value: &[u8]) -> Field {
    Field {
        name: name.to_string(),
        category,
        value: value.to_vec(),
    }
}

fn req(policy: &str, op: Operation, url: &str, fields: Vec<Field>) -> FixtureRequest {
    FixtureRequest {
        policy: policy.to_string(),
        operation: op,
        canonical_url: url.to_string(),
        fields,
    }
}

fn response(status: u16, headers: Vec<Field>, body: &[u8]) -> FixtureResponse {
    FixtureResponse {
        status,
        headers,
        body: body.to_vec(),
    }
}

fn exchange(request: FixtureRequest, reply: FixtureReply) -> FixtureExchange {
    FixtureExchange { request, reply }
}

fn accept_request() -> FixtureRequest {
    req(
        POLICY,
        Operation::Get,
        URL,
        vec![field("accept", FieldCategory::Header, b"application/json")],
    )
}

fn token_request() -> FixtureRequest {
    req(
        POLICY,
        Operation::Get,
        URL,
        vec![field("token", FieldCategory::Token, SENTINEL.as_bytes())],
    )
}

fn base_exchange() -> FixtureExchange {
    exchange(
        accept_request(),
        FixtureReply::Response(response(
            200,
            vec![field(
                "content-type",
                FieldCategory::Header,
                b"application/json",
            )],
            b"ok",
        )),
    )
}

fn sentinel_exchange(kind: &str) -> FixtureExchange {
    let reply = match kind {
        "response" => FixtureReply::Response(response(
            200,
            vec![field(
                "set-cookie",
                FieldCategory::Cookie,
                SENTINEL.as_bytes(),
            )],
            SENTINEL.as_bytes(),
        )),
        "error" => FixtureReply::Error(FixtureReplyError {
            diagnostic: SENTINEL.as_bytes().to_vec(),
        }),
        _ => unreachable!(),
    };
    exchange(token_request(), reply)
}

fn send_fail(engine: &mut FixtureEngine, actual: FixtureRequest, msg: &str) -> FixtureFailure {
    match engine.send(actual) {
        Ok(_) => panic!("{msg}"),
        Err(failure) => failure,
    }
}

fn mismatch_at_front(f: FixtureFailure, coord: FailureCoordinate) {
    assert!(f.kind == FixtureFailureKind::Mismatch, "Mismatch kind");
    assert!(f.index == 0, "index stays at front");
    assert!(f.coordinate == coord, "first coordinate {coord:?}");
}

#[test]
fn matched_replies_move_and_advance_once() {
    let mut engine = FixtureEngine::new(vec![base_exchange(), sentinel_exchange("error")]);
    let r = engine.send(accept_request()).expect("response match");
    assert!(matches!(r, FixtureReply::Response(ref r) if r.status == 200));
    let e = engine.send(token_request()).expect("error match");
    assert!(matches!(e, FixtureReply::Error(ref e) if e.diagnostic == SENTINEL.as_bytes()));
    let f = send_fail(
        &mut engine,
        req(POLICY, Operation::Get, URL, vec![]),
        "exhausted",
    );
    assert!(f.kind == FixtureFailureKind::Unexpected);
    assert!(f.index == 2);
    // Every `FixtureFailure` maps to `HostError::Fixture` (exit 6).
    let err: HostError = f.into();
    assert!(err == HostError::Fixture && err.code() == 6);
}

#[test]
fn mismatch_reports_each_first_coordinate_and_preserves_front() {
    let mut engine = FixtureEngine::new(vec![base_exchange()]);
    let mut a = accept_request();
    a.policy = "other".into();
    let f = send_fail(&mut engine, a, "policy");
    mismatch_at_front(f, FailureCoordinate::Policy);
    let mut a = accept_request();
    a.operation = Operation::PostPublicGraphql;
    let f = send_fail(&mut engine, a, "operation");
    mismatch_at_front(f, FailureCoordinate::Operation);
    let mut a = accept_request();
    a.canonical_url = "https://other.example.com/".into();
    let f = send_fail(&mut engine, a, "url");
    mismatch_at_front(f, FailureCoordinate::CanonicalUrl);
    let mut a = accept_request();
    a.fields.push(field("x", FieldCategory::Header, b"v"));
    let f = send_fail(&mut engine, a, "field count");
    mismatch_at_front(f, FailureCoordinate::FieldCount);
    let mut a = accept_request();
    a.fields[0].name = "content-type".into();
    let f = send_fail(&mut engine, a, "field name");
    mismatch_at_front(f, FailureCoordinate::FieldName(0));
    let mut a = accept_request();
    a.fields[0].category = FieldCategory::Cookie;
    let f = send_fail(&mut engine, a, "field category");
    mismatch_at_front(f, FailureCoordinate::FieldCategory(0));
    let mut a = accept_request();
    a.fields[0].value = b"text/plain".to_vec();
    let f = send_fail(&mut engine, a, "field value");
    mismatch_at_front(f, FailureCoordinate::FieldValue(0));
    assert!(
        engine.send(accept_request()).is_ok(),
        "front preserved across mismatches"
    );
}

#[test]
fn exhausted_send_is_unexpected_remaining_zero() {
    let mut engine = FixtureEngine::new(vec![base_exchange()]);
    assert!(engine.send(accept_request()).is_ok());
    let f = send_fail(
        &mut engine,
        req(POLICY, Operation::Get, URL, vec![]),
        "exhausted",
    );
    assert!(f.kind == FixtureFailureKind::Unexpected);
    assert!(f.index == 1);
    assert!(f.coordinate == FailureCoordinate::Queue { remaining: 0 });
}

#[test]
fn finish_missing_and_unconsumed_preserve_queue_for_resume() {
    let mut one = FixtureEngine::new(vec![base_exchange()]);
    let f = one.finish().expect_err("missing");
    assert!(f.kind == FixtureFailureKind::Missing);
    assert!(f.coordinate == FailureCoordinate::Queue { remaining: 1 });
    assert!(one.send(accept_request()).is_ok(), "resume after missing");
    let mut many = FixtureEngine::new(vec![base_exchange(), base_exchange()]);
    let f = many.finish().expect_err("unconsumed");
    assert!(f.kind == FixtureFailureKind::Unconsumed);
    assert!(f.index == 0);
    assert!(f.coordinate == FailureCoordinate::Queue { remaining: 2 });
    assert!(
        many.send(accept_request()).is_ok(),
        "resume after unconsumed"
    );
}

#[test]
fn finish_empty_and_repeated_are_idempotent() {
    assert!(FixtureEngine::default().finish().expect("empty").is_empty());
    let mut engine = FixtureEngine::new(vec![sentinel_exchange("response")]);
    assert!(engine.send(token_request()).is_ok());
    let a = engine.finish().expect("finish");
    let b = engine.finish().expect("finish again");
    assert!(a == b);
    assert!(a.len() == 1);
    assert!(!format!("{a:?}").contains(SENTINEL));
}

#[test]
fn response_observation_metadata_exact_and_sentinel_free() {
    let mut engine = FixtureEngine::new(vec![sentinel_exchange("response")]);
    assert!(engine.send(token_request()).is_ok());
    let o = engine.finish().expect("obs").pop().unwrap();
    assert!(o.index == 0);
    assert!(o.outcome == FixtureOutcome::Returned);
    assert!(o.response_status == Some(200));
    let h = &o.response[0];
    assert!(h.order == 0 && h.name == "set-cookie");
    assert!(h.category == FieldCategory::Cookie && h.byte_count == SENTINEL.len());
    assert!(o.response_body_byte_count == Some(SENTINEL.len()));
    assert!(o.diagnostic_byte_count.is_none());
    assert!(o.request[0].byte_count == SENTINEL.len() && o.request[0].name == "token");
    assert!(!o.policy.contains(SENTINEL) && !o.canonical_url.contains(SENTINEL));
}

#[test]
fn error_observation_omits_status_and_body_keeps_diagnostic_count() {
    let mut engine = FixtureEngine::new(vec![sentinel_exchange("error")]);
    assert!(engine.send(token_request()).is_ok());
    let o = engine.finish().expect("obs").pop().unwrap();
    assert!(o.outcome == FixtureOutcome::Errored);
    assert!(o.response_status.is_none() && o.response.is_empty());
    assert!(o.response_body_byte_count.is_none());
    assert!(o.diagnostic_byte_count == Some(SENTINEL.len()));
}

#[test]
fn all_field_categories_produce_observations() {
    let cats = [
        FieldCategory::Header,
        FieldCategory::Cookie,
        FieldCategory::Token,
        FieldCategory::Query,
        FieldCategory::Body,
        FieldCategory::Variables,
        FieldCategory::Diagnostic,
    ];
    let build = || -> Vec<Field> {
        cats.iter()
            .enumerate()
            .map(|(i, c)| field(&format!("f{i}"), *c, b"v"))
            .collect()
    };
    let mut engine = FixtureEngine::new(vec![exchange(
        req(POLICY, Operation::Get, URL, build()),
        FixtureReply::Response(response(204, vec![], b"")),
    )]);
    assert!(
        engine
            .send(req(POLICY, Operation::Get, URL, build()))
            .is_ok()
    );
    assert!(engine.finish().expect("obs")[0].request.len() == 7);
}

#[test]
fn raw_values_disposed_and_safe_debug_sentinel_free() {
    let mut engine = FixtureEngine::new(vec![sentinel_exchange("response")]);
    let mut a = accept_request();
    a.operation = Operation::PostPublicGraphql;
    let f = send_fail(&mut engine, a, "front mismatch");
    assert!(!format!("{f:?}").contains(SENTINEL));
    assert!(!format!("{engine:?}").contains(SENTINEL));
    assert!(engine.send(token_request()).is_ok());
    let obs = engine.finish().expect("obs");
    assert!(
        !format!("{obs:?}").contains(SENTINEL),
        "observation Debug sentinel-free"
    );
}

#[test]
fn reply_body_sentinel_lives_only_in_moved_return() {
    let mut engine = FixtureEngine::new(vec![sentinel_exchange("response")]);
    let reply = engine.send(token_request()).expect("match");
    let body = match reply {
        FixtureReply::Response(r) => r.body,
        _ => panic!("response"),
    };
    assert!(
        body == SENTINEL.as_bytes(),
        "moved reply retains body bytes"
    );
}

#[test]
fn sanitize_name_matrix() {
    assert!(sanitize_name("Accept") == "accept");
    assert!(sanitize_name("a") == "a");
    assert!(sanitize_name(&"a".repeat(64)) == "a".repeat(64));
    assert!(sanitize_name("0x") == "0x", "digit start allowed");
    assert!(sanitize_name("a.b-c_d0") == "a.b-c_d0");
    assert!(sanitize_name("") == "<redacted>", "empty");
    assert!(sanitize_name("Ab cd") == "<redacted>", "space");
    assert!(sanitize_name("café") == "<redacted>", "non-ascii");
    assert!(sanitize_name(&"a".repeat(65)) == "<redacted>", "overlong");
}

#[test]
fn sanitize_url_matrix() {
    assert!(sanitize_url("https://example.com/") == "https://example.com/");
    assert!(sanitize_url("https://example.com/path") == "https://example.com/path");
    assert!(sanitize_url("https://example.com/path?q=1#frag") == "https://example.com/path");
    assert!(sanitize_url("https://user:pw@example.com/") == "<redacted-url>");
    assert!(sanitize_url("http://example.com/") == "<redacted-url>");
    assert!(sanitize_url("https://example.com:8443/") == "<redacted-url>");
    assert!(sanitize_url("https://127.0.0.1/") == "<redacted-url>");
    assert!(sanitize_url("https://[::1]/") == "<redacted-url>");
    assert!(sanitize_url("not a url") == "<redacted-url>");
    let l63 = "a".repeat(63);
    assert!(
        sanitize_url(&format!("https://{l63}.example.com/"))
            == format!("https://{l63}.example.com/")
    );
    let l64 = "a".repeat(64);
    assert!(sanitize_url(&format!("https://{l64}.example.com/")) == "<redacted-url>");
    let h253 = format!(
        "{}.{}.{}.{}",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert!(h253.len() == 253);
    assert!(sanitize_url(&format!("https://{h253}/")) == format!("https://{h253}/"));
    let h254 = format!("{h253}x");
    assert!(sanitize_url(&format!("https://{h254}/")) == "<redacted-url>");
    let p256 = format!("/{}", "a".repeat(255));
    assert!(
        sanitize_url(&format!("https://example.com{p256}")) == format!("https://example.com{p256}")
    );
    let p257 = format!("/{}", "a".repeat(256));
    assert!(
        sanitize_url(&format!("https://example.com{p257}")) == "https://example.com/redacted-path"
    );
}

static_assertions::assert_not_impl_any!(FixtureRequest: Debug, Clone);
static_assertions::assert_not_impl_any!(FixtureResponse: Debug, Clone);
static_assertions::assert_not_impl_any!(Field: Debug, Clone);
static_assertions::assert_not_impl_any!(FixtureReplyError: Debug, Clone);
static_assertions::assert_not_impl_any!(FixtureReply: Debug, Clone);
static_assertions::assert_not_impl_any!(FixtureExchange: Debug, Clone);

static_assertions::assert_impl_all!(Operation: Debug, Clone, Copy, PartialEq, Eq);
static_assertions::assert_impl_all!(FieldCategory: Debug, Clone, Copy, PartialEq, Eq);
static_assertions::assert_impl_all!(FixtureOutcome: Debug, Clone, Copy, PartialEq, Eq);
static_assertions::assert_impl_all!(FixtureFailure: Debug, Clone, PartialEq, Eq);
static_assertions::assert_impl_all!(FixtureFailureKind: Debug, Clone, Copy, PartialEq, Eq);
static_assertions::assert_impl_all!(FailureCoordinate: Debug, Clone, PartialEq, Eq);
static_assertions::assert_impl_all!(Observation: Debug, Clone, PartialEq, Eq);
static_assertions::assert_impl_all!(ObservedField: Debug, Clone, PartialEq, Eq);

//! CLI behavior matrix for the test-only BEX host. These tests own the
//! deterministic-CLI acceptance criteria: grammar, field order, stream
//! separation, exit codes, command nulling, and the single trailing LF.

use bex_test_host::contracts::{
    Command, Execution, HostError, HostResult, PackagePayload, Report, WitBinding, codes,
};
use bex_test_host::output::{Format, exit_code, render};
use std::path::PathBuf;
use std::process::Command as Process;

const BIN: &str = env!("CARGO_BIN_EXE_bex-test-host");

fn package_result() -> HostResult {
    HostResult {
        package: PackagePayload {
            id: "media-url-resolver.infinity-factory.direct-url".into(),
            kind: "media-url-resolver".into(),
            version: "1".into(),
            asset_name: "direct-url.bex".into(),
            asset_sha256: "deadbeef".into(),
            asset_size: 7,
            wit: WitBinding {
                package: "component:media-url-resolver".into(),
                version: "1.0.0".into(),
                world: "media-url-resolver".into(),
                sha256: "feedface".into(),
            },
            network_policy: None,
            inventory: vec![
                "manifest.json".into(),
                "plugin.wasm".into(),
                "plugin.wit".into(),
            ],
        },
        unprovided_imports: vec![],
        execution: Execution {
            supported: false,
            status: "deferred".into(),
        },
    }
}

#[test]
fn json_success_has_exact_field_order_and_one_lf() {
    let report = Report::ok(Command::Validate, package_result());
    let bytes = render(&report, Format::Json);
    let text = std::str::from_utf8(&bytes).expect("json must be utf8");
    assert_eq!(
        text.as_bytes().last(),
        Some(&b'\n'),
        "exactly one trailing LF"
    );
    assert_eq!(text.matches('\n').count(), 1, "no interior newlines");
    let order: Vec<&str> = [
        "schema_version",
        "test_only",
        "production_host",
        "status",
        "command",
        "result",
        "error",
    ]
    .into_iter()
    .collect();
    let positions = order
        .iter()
        .map(|key| {
            text.find(&format!("\"{key}\""))
                .expect("field must be present")
        })
        .collect::<Vec<_>>();
    let mut sorted = positions.clone();
    sorted.sort_unstable();
    assert_eq!(positions, sorted, "top-level field order must be exact");
    assert!(text.contains("\"test_only\":true"));
    assert!(text.contains("\"production_host\":false"));
    assert!(text.contains("\"status\":\"ok\""));
    assert!(text.contains("\"command\":\"validate\""));
    assert!(text.contains("\"error\":null"));
    assert!(text.contains("\"supported\":false"));
    assert!(text.contains("\"status\":\"deferred\""));
}

#[test]
fn text_success_uses_dotted_fields_json_arrays_and_one_lf() {
    let report = Report::ok(Command::Inspect, package_result());
    let bytes = render(&report, Format::Text);
    let text = std::str::from_utf8(&bytes).expect("text must be utf8");
    assert_eq!(text.as_bytes().last(), Some(&b'\n'));
    assert!(text.contains("schema_version=1"));
    assert!(text.contains("test_only=true"));
    assert!(text.contains("production_host=false"));
    assert!(text.contains("status=ok"));
    assert!(text.contains("command=inspect"));
    assert!(text.contains("error=null"));
    assert!(text.contains("result.package.id=media-url-resolver.infinity-factory.direct-url"));
    assert!(text.contains("result.package.wit.sha256=feedface"));
    assert!(text.contains("result.execution.supported=false"));
    assert!(text.contains("result.execution.status=deferred"));
    assert!(
        text.contains(r#"result.package.inventory=["manifest.json","plugin.wasm","plugin.wit"]"#)
    );
    assert_eq!(exit_code(&report), codes::SUCCESS);
}

#[test]
fn json_error_preserves_command_and_stream_separator() {
    let report = Report::error(Some(Command::Validate), HostError::Runtime);
    let bytes = render(&report, Format::Json);
    let text = std::str::from_utf8(&bytes).expect("json must be utf8");
    assert!(text.contains("\"status\":\"error\""));
    assert!(text.contains("\"command\":\"validate\""));
    assert!(text.contains("\"result\":null"));
    assert!(text.contains("\"error\":{\"code\":4,\"kind\":\"runtime\""));
    assert_eq!(exit_code(&report), codes::RUNTIME);
}

#[test]
fn text_usage_error_nulls_command_and_exits_two() {
    let report = Report::error(None, HostError::Usage);
    let bytes = render(&report, Format::Text);
    let text = std::str::from_utf8(&bytes).expect("text must be utf8");
    assert!(text.contains("status=error"));
    assert!(text.contains("command=null"));
    assert!(text.contains("result=null"));
    assert!(text.contains("error.code=2"));
    assert!(text.contains("error.kind=usage"));
    assert_eq!(exit_code(&report), codes::USAGE);
}

#[test]
fn cli_bogus_verb_is_usage_exit_two_on_stderr() {
    let output = Process::new(BIN)
        .arg("bogus")
        .arg("--json")
        .output()
        .expect("bin must run");
    assert!(!output.status.success(), "usage must not exit zero");
    assert_eq!(output.status.code(), Some(codes::USAGE as i32));
    assert!(output.stdout.is_empty(), "usage must write only to stderr");
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr must be utf8");
    assert!(stderr.contains("\"command\":null"));
    assert!(stderr.contains("\"error\":{\"code\":2,\"kind\":\"usage\""));
    assert_eq!(stderr.as_bytes().last(), Some(&b'\n'));
}

#[test]
fn cli_validate_real_writes_success_to_stdout_and_is_byte_repeatable() {
    let package = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/packages/direct-url.bex");
    let first = Process::new(BIN)
        .args(["validate", package.to_str().unwrap(), "--json"])
        .output()
        .expect("bin must run");
    assert_eq!(
        first.status.code(),
        Some(codes::SUCCESS as i32),
        "valid package must exit 0"
    );
    assert!(
        first.stderr.is_empty(),
        "success must write only to stdout, never stderr"
    );
    let second = Process::new(BIN)
        .args(["validate", package.to_str().unwrap(), "--json"])
        .output()
        .expect("bin must run");
    assert_eq!(
        first.stdout, second.stdout,
        "output must be byte-repeatable"
    );
    let stdout = std::str::from_utf8(&first.stdout).expect("stdout must be utf8");
    assert!(stdout.contains("\"status\":\"ok\""));
    assert!(stdout.contains("\"command\":\"validate\""));
    assert!(stdout.contains("\"error\":null"));
}

#[test]
fn cli_validate_missing_package_exits_three_on_stderr() {
    let output = Process::new(BIN)
        .args(["validate", "/nonexistent/pkg.bex", "--json"])
        .output()
        .expect("bin must run");
    assert_eq!(output.status.code(), Some(codes::PACKAGE as i32));
    assert!(
        output.stdout.is_empty(),
        "package error must write only to stderr"
    );
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr must be utf8");
    assert!(stderr.contains("\"command\":\"validate\""));
    assert!(stderr.contains("\"error\":{\"code\":3,\"kind\":\"package\""));
}

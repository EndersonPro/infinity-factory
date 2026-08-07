//! Child-process boundary proof for the BEX test-host fixture failure.
//!
//! `harness = false`: the parent/JV modes are chosen by a private environment
//! marker so no CLI verb, libtest harness, or runtime/component path is
//! exercised. The child renders the generic `Report::error(None,
//! HostError::Fixture)` to stderr and exits `6`; the parent requires empty
//! stdout, an exact stderr document, exit code `6`, and the absence of any
//! fixture sentinel in either stream. No CLI grammar change and no smoke/doc
//! coverage here.

use std::io::Write;
use std::process::Command;

use bex_test_host::contracts::{HostError, Report};
use bex_test_host::output::{Format, exit_code, render};

/// Environment marker that switches the same test binary into child mode.
const CHILD_MARK: &str = "BEX_TEST_HOST_FIXTURE_CHILD";
/// Sentinel deliberately injected into the child environment; it MUST never
/// reach stdout or stderr because the generic failure rendering ignores it.
const SENTINEL: &str = "SECRET-s3ntinel-VALue!";

const EXPECTED_STDERR: &str = "{\"schema_version\":\"1\",\"test_only\":true,\"production_host\":false,\"status\":\"error\",\"command\":null,\"result\":null,\"error\":{\"code\":6,\"kind\":\"fixture_mismatch\",\"message\":\"fixture validation failed\"}}\n";

fn main() {
    if std::env::var_os(CHILD_MARK).is_some() {
        child();
        return;
    }
    parent();
}

fn child() {
    let report = Report::error(None, HostError::Fixture);
    let bytes = render(&report, Format::Json);
    let _ = std::io::stderr().write_all(&bytes);
    let _ = std::io::stderr().flush();
    std::process::exit(exit_code(&report) as i32);
}

fn parent() {
    let exe = std::env::current_exe().expect("current_exe");
    let output = Command::new(exe)
        .env(CHILD_MARK, "1")
        .env("BEX_FIXTURE_SENTINEL", SENTINEL)
        .output()
        .expect("child process must run");
    assert!(
        output.status.code() == Some(6),
        "fixture failure must exit 6, got {:?}",
        output.status.code()
    );
    assert!(
        output.stdout.is_empty(),
        "child must write nothing to stdout"
    );
    let stderr = std::str::from_utf8(&output.stderr).expect("stderr must be utf8");
    assert_eq!(stderr, EXPECTED_STDERR, "exact fixture stderr document");
    assert!(
        !stderr.contains(SENTINEL),
        "sentinel must not appear in stderr"
    );
    let stdout = std::str::from_utf8(&output.stdout).unwrap_or("");
    assert!(
        !stdout.contains(SENTINEL),
        "sentinel must not appear in stdout"
    );
    // Visible evidence that every child assertion executed and passed.
    eprintln!(
        "fixture_process: child boundary verified (exit 6, empty stdout, exact stderr, no sentinel)"
    );
}

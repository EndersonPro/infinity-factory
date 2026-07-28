//! Public contract types for the test-only BEX host. Field order is fixed by
//! the design's Result Contract and is preserved verbatim by `serde` for the
//! JSON renderer and by the dotted text renderer.

use serde::Serialize;

pub const SCHEMA_VERSION: &str = "1";

/// The host CLI grammar accepts exactly these two verbs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Validate,
    Inspect,
}

impl Command {
    pub fn as_str(self) -> &'static str {
        match self {
            Command::Validate => "validate",
            Command::Inspect => "inspect",
        }
    }
}

/// Upper-level `Result.payload.wit` object: canonical WIT identity bound to a
/// packaged plugin. Field order MUST stay `package, version, world, sha256`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WitBinding {
    pub package: String,
    pub version: String,
    pub world: String,
    pub sha256: String,
}

/// `Result.payload.inventory` lists archive member names in canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Execution {
    pub supported: bool,
    pub status: String,
}

/// `Result.payload` (the nested `package` object plus inspection fields).
/// Field order is `id, type, version, asset_name, asset_sha256, asset_size,
/// wit, network_policy, inventory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PackagePayload {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub version: String,
    pub asset_name: String,
    pub asset_sha256: String,
    pub asset_size: u64,
    pub wit: WitBinding,
    pub network_policy: Option<String>,
    pub inventory: Vec<String>,
}

/// The whole `result` object: `package`, `unprovided_imports`, `execution`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostResult {
    pub package: PackagePayload,
    pub unprovided_imports: Vec<String>,
    pub execution: Execution,
}

/// Reserved exit codes. The host MUST use only these.
pub mod codes {
    pub const SUCCESS: u8 = 0;
    pub const USAGE: u8 = 2;
    pub const PACKAGE: u8 = 3;
    pub const RUNTIME: u8 = 4;
    pub const POLICY: u8 = 5;
    pub const FIXTURE: u8 = 6;
    pub const GATE: u8 = 7;
}

/// Every failure the host can report. `code` maps to the exit code and the
/// emitted `error.code` field; `kind` is the stable machine identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    Usage,
    Package,
    Runtime,
    Policy,
    Fixture,
    Gate,
}

impl HostError {
    pub fn code(&self) -> u8 {
        match self {
            HostError::Usage => codes::USAGE,
            HostError::Package => codes::PACKAGE,
            HostError::Runtime => codes::RUNTIME,
            HostError::Policy => codes::POLICY,
            HostError::Fixture => codes::FIXTURE,
            HostError::Gate => codes::GATE,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            HostError::Usage => "usage",
            HostError::Package => "package",
            HostError::Runtime => "runtime",
            HostError::Policy => "policy",
            HostError::Fixture => "fixture_mismatch",
            HostError::Gate => "output_gate",
        }
    }

    pub fn message(&self) -> &'static str {
        match self {
            HostError::Usage => "invalid command line",
            HostError::Package => "package, catalog, or contract validation failed",
            HostError::Runtime => "runtime execution is deferred",
            HostError::Policy => "policy or network validation failed",
            HostError::Fixture => "fixture validation failed",
            HostError::Gate => "output or download gate failed",
        }
    }

    /// Sentinel for seams that are not yet wired. Runtime is the honest home:
    /// the test-only host defers every execution/code path it has not proven.
    pub fn not_implemented() -> Self {
        HostError::Runtime
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ok,
    Error,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Error => "error",
        }
    }
}

/// The full report the host renders. Field order is fixed:
/// `schema_version, test_only, production_host, status, command, result, error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub status: Status,
    pub command: Option<Command>,
    pub result: Option<HostResult>,
    pub error: Option<HostError>,
}

impl Report {
    pub const fn ok(command: Command, result: HostResult) -> Self {
        Report {
            status: Status::Ok,
            command: Some(command),
            result: Some(result),
            error: None,
        }
    }

    pub const fn error(command: Option<Command>, error: HostError) -> Self {
        Report {
            status: Status::Error,
            command,
            result: None,
            error: Some(error),
        }
    }
}

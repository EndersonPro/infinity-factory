//! Test-only BEX host: deterministic `validate`/`inspect`, stream separation,
//! and stable exits. This crate never invokes a runtime, opens a socket, or
//! downloads anything; packages are inspected bytes, never executed.

pub mod contracts;
pub mod fixture;
pub mod output;
pub mod package;

pub use contracts::{Command, HostError, HostResult, PackagePayload, Report, Status, WitBinding};
pub use fixture::{
    Field, FieldCategory, FixtureEngine, FixtureRequest, FixtureResponse, Operation,
};
pub use output::{Format, exit_code, render};
pub use package::{inspect_package, validate_package};

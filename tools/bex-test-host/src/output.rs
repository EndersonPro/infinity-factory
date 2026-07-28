//! Output renderer and exit adapter. One renderer produces text or JSON from
//! a `Report`; the exit adapter maps the report to its reserved exit code.
//! Field order is fixed by the design's Result Contract and is reproduced
//! verbatim in both formats.

use crate::contracts::{Command, HostError, Report};

pub enum Format {
    Text,
    Json,
}

pub fn render(report: &Report, format: Format) -> Vec<u8> {
    match format {
        Format::Json => render_json(report),
        Format::Text => render_text(report),
    }
}

pub fn exit_code(report: &Report) -> u8 {
    report.error.map_or(0, |error| error.code())
}

fn command_token(command: Option<Command>) -> &'static str {
    match command {
        None => "null",
        Some(Command::Validate) => "validate",
        Some(Command::Inspect) => "inspect",
    }
}

fn render_json(report: &Report) -> Vec<u8> {
    let mut out = String::from(
        "{\"schema_version\":\"1\",\"test_only\":true,\"production_host\":false,\"status\":\"",
    );
    out.push_str(report.status.as_str());
    out.push_str("\",\"command\":");
    match report.command {
        None => out.push_str("null"),
        Some(command) => {
            out.push('"');
            out.push_str(command.as_str());
            out.push('"');
        }
    }
    out.push_str(",\"result\":");
    match &report.result {
        None => out.push_str("null"),
        Some(result) => {
            out.push_str(&serde_json::to_string(result).expect("result must serialize"))
        }
    }
    out.push_str(",\"error\":");
    match &report.error {
        None => out.push_str("null"),
        Some(error) => out.push_str(&error_json(error)),
    }
    out.push_str("}\n");
    out.into_bytes()
}

fn error_json(error: &HostError) -> String {
    format!(
        "{{\"code\":{},\"kind\":\"{}\",\"message\":\"{}\"}}",
        error.code(),
        error.kind(),
        escape(error.message())
    )
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn render_text(report: &Report) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();
    lines.push("schema_version=1".into());
    lines.push("test_only=true".into());
    lines.push("production_host=false".into());
    lines.push(format!("status={}", report.status.as_str()));
    lines.push(format!("command={}", command_token(report.command)));
    match &report.result {
        None => lines.push("result=null".into()),
        Some(result) => {
            let package = &result.package;
            lines.push(format!("result.package.id={}", package.id));
            lines.push(format!("result.package.type={}", package.kind));
            lines.push(format!("result.package.version={}", package.version));
            lines.push(format!("result.package.asset_name={}", package.asset_name));
            lines.push(format!(
                "result.package.asset_sha256={}",
                package.asset_sha256
            ));
            lines.push(format!("result.package.asset_size={}", package.asset_size));
            lines.push(format!(
                "result.package.wit.package={}",
                package.wit.package
            ));
            lines.push(format!(
                "result.package.wit.version={}",
                package.wit.version
            ));
            lines.push(format!("result.package.wit.world={}", package.wit.world));
            lines.push(format!("result.package.wit.sha256={}", package.wit.sha256));
            lines.push(format!(
                "result.package.network_policy={}",
                package.network_policy.as_deref().unwrap_or("null")
            ));
            lines.push(format!(
                "result.package.inventory={}",
                serde_json::to_string(&package.inventory).expect("inventory must serialize")
            ));
            lines.push(format!(
                "result.unprovided_imports={}",
                serde_json::to_string(&result.unprovided_imports).expect("imports must serialize")
            ));
            lines.push(format!(
                "result.execution.supported={}",
                result.execution.supported
            ));
            lines.push(format!(
                "result.execution.status={}",
                result.execution.status
            ));
        }
    }
    match &report.error {
        None => lines.push("error=null".into()),
        Some(error) => {
            lines.push(format!("error.code={}", error.code()));
            lines.push(format!("error.kind={}", error.kind()));
            lines.push(format!("error.message={}", error.message()));
        }
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out.into_bytes()
}

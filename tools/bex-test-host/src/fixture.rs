//! Standalone fixture contract seams: typed ordered request/response/error/
//! observation with `send`/`finish`. Stage 1 returns honest
//! `Runtime::not_implemented` until WU4 wires exact byte/vector matching and
//! redaction.

use crate::contracts::HostError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Get,
    PostPublicGraphql,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldCategory {
    Header,
    Cookie,
    Token,
    Query,
    Body,
    Variables,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub category: FieldCategory,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureRequest {
    pub policy: String,
    pub operation: Operation,
    pub canonical_url: String,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureResponse {
    pub status: u16,
    pub headers: Vec<Field>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FixtureOutcome {
    Returned,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub name: String,
    pub category: FieldCategory,
    pub byte_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub index: usize,
    pub policy: String,
    pub operation: Operation,
    pub canonical_url: String,
    pub request: Vec<Observed>,
    pub response_status: Option<u16>,
    pub response: Vec<Observed>,
    pub outcome: FixtureOutcome,
}

/// Sealed fixture engine. Every matcher operation is deferred until WU4.
pub struct FixtureEngine;

impl Default for FixtureEngine {
    fn default() -> Self {
        FixtureEngine
    }
}

impl FixtureEngine {
    pub fn new() -> Self {
        FixtureEngine
    }

    pub fn send(
        &mut self,
        _request: &FixtureRequest,
        _response: &FixtureResponse,
    ) -> Result<Observation, HostError> {
        Err(HostError::not_implemented())
    }

    pub fn finish(&mut self) -> Result<Vec<Observation>, HostError> {
        Err(HostError::not_implemented())
    }
}

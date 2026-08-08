//! Standalone fixture contract: an owned, ordered, secret-safe request/reply
//! matcher. Raw bytes live only in pending exchanges, the by-value actual
//! request, and the moved reply; failures and observations carry sanitized
//! metadata only. Every `FixtureFailure` maps to `HostError::Fixture`.

use std::collections::VecDeque;
use std::fmt;

use crate::contracts::HostError;
use url::Url;

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

/// Raw-bearing field: name + category + value bytes. No `Debug`/`Clone`.
#[derive(PartialEq, Eq)]
pub struct Field {
    pub name: String,
    pub category: FieldCategory,
    pub value: Vec<u8>,
}

/// Raw-bearing request. By value when sent. No `Debug`/`Clone`.
#[derive(PartialEq, Eq)]
pub struct FixtureRequest {
    pub policy: String,
    pub operation: Operation,
    pub canonical_url: String,
    pub fields: Vec<Field>,
}

/// Raw-bearing response: status + header fields + body bytes. No `Debug`/`Clone`.
#[derive(PartialEq, Eq)]
pub struct FixtureResponse {
    pub status: u16,
    pub headers: Vec<Field>,
    pub body: Vec<u8>,
}

/// Raw-bearing reply error: diagnostic bytes only. No `Debug`/`Clone`.
#[derive(PartialEq, Eq)]
pub struct FixtureReplyError {
    pub diagnostic: Vec<u8>,
}

/// Configured reply: response or error. Raw-bearing. No `Debug`/`Clone`.
#[derive(PartialEq, Eq)]
pub enum FixtureReply {
    Response(FixtureResponse),
    Error(FixtureReplyError),
}

/// One expected exchange: a request paired with its reply. No `Debug`/`Clone`.
pub struct FixtureExchange {
    pub request: FixtureRequest,
    pub reply: FixtureReply,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureOutcome {
    Returned,
    Errored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedField {
    pub order: usize,
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
    pub request: Vec<ObservedField>,
    pub response_status: Option<u16>,
    pub response: Vec<ObservedField>,
    pub response_body_byte_count: Option<usize>,
    pub outcome: FixtureOutcome,
    pub diagnostic_byte_count: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixtureFailureKind {
    Mismatch,
    Unexpected,
    Missing,
    Unconsumed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureCoordinate {
    Policy,
    Operation,
    CanonicalUrl,
    FieldCount,
    FieldName(usize),
    FieldCategory(usize),
    FieldValue(usize),
    Queue { remaining: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureFailure {
    pub kind: FixtureFailureKind,
    pub index: usize,
    pub coordinate: FailureCoordinate,
}

impl FixtureFailure {
    pub fn new(kind: FixtureFailureKind, index: usize, coordinate: FailureCoordinate) -> Self {
        FixtureFailure {
            kind,
            index,
            coordinate,
        }
    }
}

impl From<FixtureFailure> for HostError {
    fn from(_failure: FixtureFailure) -> Self {
        HostError::Fixture
    }
}

/// Secret-safe ordered fixture engine. Safe `Debug` never prints raw bytes.
#[derive(Default)]
pub struct FixtureEngine {
    pending: VecDeque<FixtureExchange>,
    next_index: usize,
    observations: Vec<Observation>,
}

impl fmt::Debug for FixtureEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FixtureEngine")
            .field("pending", &"<redacted>")
            .field("next_index", &self.next_index)
            .field("observations_len", &self.observations.len())
            .finish()
    }
}

impl FixtureEngine {
    pub fn new(exchanges: Vec<FixtureExchange>) -> Self {
        FixtureEngine {
            pending: exchanges.into(),
            next_index: 0,
            observations: Vec::new(),
        }
    }

    /// Compare only the next expectation. Raw bytes leave only the by-value
    /// actual request (dropped here) and the moved reply.
    pub fn send(&mut self, actual: FixtureRequest) -> Result<FixtureReply, FixtureFailure> {
        if let Some(front) = self.pending.front() {
            if let Some(coordinate) = first_mismatch(&front.request, &actual) {
                // Mismatch: preserve the front, drop the actual request.
                drop(actual);
                return Err(FixtureFailure::new(
                    FixtureFailureKind::Mismatch,
                    self.next_index,
                    coordinate,
                ));
            }
        } else {
            // Exhausted: queue empty, state unchanged.
            drop(actual);
            return Err(FixtureFailure::new(
                FixtureFailureKind::Unexpected,
                self.next_index,
                FailureCoordinate::Queue { remaining: 0 },
            ));
        }

        // Match: pop the exchange, derive safe metadata, drop both requests,
        // and move the reply out. The index advances exactly once.
        let exchange = self
            .pending
            .pop_front()
            .expect("front checked non-empty above");
        let index = self.next_index;
        self.next_index += 1;
        let observation = observe(index, &exchange);
        self.observations.push(observation);
        // `exchange.request` and `actual` (already dropped on mismatch; here
        // `actual` is dropped at scope end) are released; raw bytes live only
        // in the moved `exchange.reply`.
        let reply = exchange.reply;
        drop(actual);
        drop(exchange.request);
        Ok(reply)
    }

    /// Empty queue yields ordered safe observations and stays idempotent. One
    /// pending is `Missing`; multiple are `Unconsumed` with the first index and
    /// exact count. A failed finish never mutates state.
    pub fn finish(&mut self) -> Result<Vec<Observation>, FixtureFailure> {
        match self.pending.len() {
            0 => Ok(self.observations.clone()),
            1 => Err(FixtureFailure::new(
                FixtureFailureKind::Missing,
                self.next_index,
                FailureCoordinate::Queue { remaining: 1 },
            )),
            remaining => Err(FixtureFailure::new(
                FixtureFailureKind::Unconsumed,
                self.next_index,
                FailureCoordinate::Queue { remaining },
            )),
        }
    }
}

fn first_mismatch(expected: &FixtureRequest, actual: &FixtureRequest) -> Option<FailureCoordinate> {
    if expected.policy != actual.policy {
        return Some(FailureCoordinate::Policy);
    }
    if expected.operation != actual.operation {
        return Some(FailureCoordinate::Operation);
    }
    if expected.canonical_url != actual.canonical_url {
        return Some(FailureCoordinate::CanonicalUrl);
    }
    if expected.fields.len() != actual.fields.len() {
        return Some(FailureCoordinate::FieldCount);
    }
    for (i, (e, a)) in expected.fields.iter().zip(actual.fields.iter()).enumerate() {
        if e.name != a.name {
            return Some(FailureCoordinate::FieldName(i));
        }
        if e.category != a.category {
            return Some(FailureCoordinate::FieldCategory(i));
        }
        if e.value != a.value {
            return Some(FailureCoordinate::FieldValue(i));
        }
    }
    None
}

fn observe(index: usize, exchange: &FixtureExchange) -> Observation {
    let policy = sanitize_name(&exchange.request.policy);
    let canonical_url = sanitize_url(&exchange.request.canonical_url);
    let request = exchange
        .request
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| ObservedField {
            order: i,
            name: sanitize_name(&f.name),
            category: f.category,
            byte_count: f.value.len(),
        })
        .collect::<Vec<_>>();
    match &exchange.reply {
        FixtureReply::Response(r) => {
            let response = r
                .headers
                .iter()
                .enumerate()
                .map(|(i, f)| ObservedField {
                    order: i,
                    name: sanitize_name(&f.name),
                    category: f.category,
                    byte_count: f.value.len(),
                })
                .collect::<Vec<_>>();
            Observation {
                index,
                policy,
                operation: exchange.request.operation,
                canonical_url,
                request,
                response_status: Some(r.status),
                response,
                response_body_byte_count: Some(r.body.len()),
                outcome: FixtureOutcome::Returned,
                diagnostic_byte_count: None,
            }
        }
        FixtureReply::Error(e) => Observation {
            index,
            policy,
            operation: exchange.request.operation,
            canonical_url,
            request,
            response_status: None,
            response: Vec::new(),
            response_body_byte_count: None,
            outcome: FixtureOutcome::Errored,
            diagnostic_byte_count: Some(e.diagnostic.len()),
        },
    }
}

/// ASCII-lowercase the name; keep it only when it matches
/// `[a-z0-9][a-z0-9._-]{0,63}`, otherwise redact wholesale.
pub fn sanitize_name(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if is_valid_name(&lower) {
        lower
    } else {
        String::from("<redacted>")
    }
}

fn is_valid_name(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 64 {
        return false;
    }
    if !is_name_first(bytes[0]) {
        return false;
    }
    bytes[1..].iter().all(|&b| is_name_rest(b))
}

fn is_name_first(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit()
}

fn is_name_rest(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
}

/// Canonical HTTPS DNS authority without userinfo, port, or IP; query and
/// fragment are removed. Invalid origins become `<redacted-url>`; an oversized
/// or invalid path becomes `https://{host}/redacted-path`.
pub fn sanitize_url(input: &str) -> String {
    let Ok(mut url) = Url::parse(input) else {
        return String::from("<redacted-url>");
    };
    if url.scheme() != "https" {
        return String::from("<redacted-url>");
    }
    if !url.username().is_empty() || url.password().is_some() {
        return String::from("<redacted-url>");
    }
    if url.port().is_some() {
        return String::from("<redacted-url>");
    }
    let Some(host) = url.host() else {
        return String::from("<redacted-url>");
    };
    let url::Host::Domain(domain) = host else {
        return String::from("<redacted-url>");
    };
    let host = domain.to_string();
    if host.is_empty() || host.len() > 253 {
        return String::from("<redacted-url>");
    }
    for label in host.split('.') {
        if label.is_empty() || label.len() > 63 {
            return String::from("<redacted-url>");
        }
    }
    url.set_query(None);
    url.set_fragment(None);
    let path = url.path().to_string();
    if path.len() > 256 {
        return format!("https://{host}/redacted-path");
    }
    format!("https://{host}{path}")
}

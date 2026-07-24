use super::{GetRequest, Header, HttpsClient, HttpsError, HttpsResponse, PublicGraphqlCall};
use std::collections::VecDeque;

pub struct PublicGraphqlExpectation {
    url: String,
    friendly_name: String,
    doc_id: String,
    variables: String,
    lsd_len: usize,
    result: Result<HttpsResponse, HttpsError>,
}
impl PublicGraphqlExpectation {
    pub fn new(
        url: &str,
        friendly_name: &str,
        doc_id: &str,
        variables: &str,
        lsd_len: usize,
        result: Result<HttpsResponse, HttpsError>,
    ) -> Self {
        Self {
            url: url.into(),
            friendly_name: friendly_name.into(),
            doc_id: doc_id.into(),
            variables: variables.into(),
            lsd_len,
            result,
        }
    }
}
pub enum ExpectedCall {
    Get(GetRequest, Result<HttpsResponse, HttpsError>),
    Graphql(PublicGraphqlExpectation),
}
#[derive(Debug, Eq, PartialEq)]
pub struct Observation {
    pub operation: &'static str,
}
pub struct MockHttpsClient {
    expected: VecDeque<ExpectedCall>,
    observations: Vec<Observation>,
    mismatch: bool,
}
impl MockHttpsClient {
    pub fn new(expected: Vec<ExpectedCall>) -> Self {
        Self {
            expected: expected.into(),
            observations: vec![],
            mismatch: false,
        }
    }
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }
    pub fn verify(&self) -> Result<(), &'static str> {
        if self.mismatch {
            Err("HTTPS call did not match expectation")
        } else if self.expected.is_empty() {
            Ok(())
        } else {
            Err("expected HTTPS calls remain unconsumed")
        }
    }
    fn reject<T>(&mut self) -> Result<T, HttpsError> {
        self.mismatch = true;
        Err(HttpsError::InvalidRequest)
    }
}
fn same_headers(left: &[Header], right: &[Header]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| left.name == right.name && left.value == right.value)
}
impl HttpsClient for MockHttpsClient {
    fn get(&mut self, request: GetRequest) -> Result<HttpsResponse, HttpsError> {
        self.observations.push(Observation { operation: "get" });
        match self.expected.pop_front() {
            Some(ExpectedCall::Get(expected, result))
                if expected.url == request.url
                    && same_headers(&expected.headers, &request.headers) =>
            {
                result
            }
            _ => self.reject(),
        }
    }
    fn post_public_graphql(
        &mut self,
        request: PublicGraphqlCall,
    ) -> Result<HttpsResponse, HttpsError> {
        self.observations.push(Observation {
            operation: "post-public-graphql",
        });
        match self.expected.pop_front() {
            Some(ExpectedCall::Graphql(expected))
                if expected.url == request.0.url
                    && expected.friendly_name == request.0.friendly_name
                    && expected.doc_id == request.0.doc_id
                    && expected.variables == request.0.variables
                    && expected.lsd_len == request.0.lsd.len() =>
            {
                expected.result
            }
            _ => self.reject(),
        }
    }
}

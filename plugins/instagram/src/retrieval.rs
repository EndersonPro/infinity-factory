use crate::{classify_url, error, extract_page_state};
use bex_media_url_resolver_v2::{
    GetRequest, HttpsClient, ResolverError, build_graphql_request, validate_https_response,
};
use serde_json::Value;
use std::fmt;

const CONFIG: &str = include_str!("../fixtures/retrieval-config.json");
const REVISION: &str = "yt-dlp-f49b551-public-v1";
const ENDPOINT: &str = "https://www.instagram.com/api/graphql";
const FRIENDLY_NAME: &str = "PolarisLoggedOutDesktopWWWPostRootContentQuery";
const DOC_ID: &str = "27130156389949648";
const VARIABLES_KEY: &str = "media_id";
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
struct RetrievalConfig {
    endpoint: String,
    friendly_name: String,
    doc_id: String,
}
fn parse_config(input: &str) -> Result<RetrievalConfig, ResolverError> {
    let value: Value = serde_json::from_str(input).map_err(|_| error::policy())?;
    let field = |name| {
        value
            .get(name)
            .and_then(Value::as_str)
            .ok_or_else(error::policy)
    };
    let revision = field("revision")?;
    let endpoint = field("endpoint")?;
    let friendly_name = field("friendly_name")?;
    let doc_id = field("doc_id")?;
    let variables_key = field("variables_key")?;
    if revision != REVISION
        || endpoint != ENDPOINT
        || friendly_name != FRIENDLY_NAME
        || doc_id != DOC_ID
        || variables_key != VARIABLES_KEY
        || doc_id.len() > 64
        || !doc_id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(error::policy());
    }
    Ok(RetrievalConfig {
        endpoint: endpoint.into(),
        friendly_name: friendly_name.into(),
        doc_id: doc_id.into(),
    })
}
pub fn validate_retrieval_config(input: &str) -> Result<(), ResolverError> {
    parse_config(input).map(|_| ())
}
fn media_id(shortcode: &str) -> Result<String, ResolverError> {
    let mut decimal = vec![0_u8];
    for byte in shortcode.bytes() {
        let mut carry = ALPHABET
            .iter()
            .position(|item| *item == byte)
            .ok_or_else(error::invalid_input)?;
        for digit in &mut decimal {
            let value = *digit as usize * 64 + carry;
            *digit = (value % 10) as u8;
            carry = value / 10;
        }
        while carry > 0 {
            decimal.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    Ok(decimal
        .into_iter()
        .rev()
        .map(|digit| char::from(b'0' + digit))
        .collect())
}
pub struct RetrievalResult {
    graphql_body: Vec<u8>,
    relay_json: Option<String>,
}
impl RetrievalResult {
    pub fn graphql_body(&self) -> &[u8] {
        &self.graphql_body
    }
    pub fn relay_json(&self) -> Option<&str> {
        self.relay_json.as_deref()
    }
}
impl fmt::Debug for RetrievalResult {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("RetrievalResult")
            .field("graphql_bytes", &self.graphql_body.len())
            .field("relay_present", &self.relay_json.is_some())
            .finish()
    }
}
pub fn retrieve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<RetrievalResult, ResolverError> {
    let canonical = classify_url(source).map_err(|_| error::invalid_input())?;
    let config = parse_config(CONFIG)?;
    let identifier = canonical
        .as_str()
        .split('/')
        .nth(4)
        .ok_or_else(error::invalid_input)?;
    let variables = format!(r#"{{"media_id":"{}"}}"#, media_id(identifier)?);
    let page = client
        .get(GetRequest {
            url: canonical.as_str().into(),
            headers: vec![],
        })
        .map_err(error::transport)?;
    validate_https_response(&page).map_err(|_| error::malformed())?;
    error::status(page.status)?;
    let state = extract_page_state(&page.body).map_err(|_| error::malformed())?;
    let (lsd, relay_json) = state.take();
    let request = build_graphql_request(
        &config.endpoint,
        lsd,
        &config.friendly_name,
        &config.doc_id,
        &variables,
    )
    .map_err(|_| error::policy())?;
    let graphql = client
        .post_public_graphql(request)
        .map_err(error::transport)?;
    validate_https_response(&graphql).map_err(|_| error::malformed())?;
    error::status(graphql.status)?;
    Ok(RetrievalResult {
        graphql_body: graphql.body,
        relay_json,
    })
}

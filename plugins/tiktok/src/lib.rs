mod retrieval;
mod url;

// `error` helpers are exercised by retrieval (invalid_input, transport,
// malformed, status and the private/unavailable arms status reaches) and,
// once payload lands, `unsupported`. Until then `unsupported` is the only
// uncalled helper; its `dead_code` is suppressed at the function boundary
// and removed when payload.rs maps a non-zero statusCode to it.
mod error;

pub use retrieval::{retrieve_https, retrieve_source};
pub use url::{CanonicalUrl, classify_url};
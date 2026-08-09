mod payload;
mod retrieval;
mod url;

// `error` helpers are exercised by retrieval (invalid_input, transport,
// malformed, status and the private/unavailable arms status reaches) and by
// payload (unsupported for non-zero statusCode and missing/malformed video).
mod error;

pub use payload::{parse_universal_data, VideoData};
pub use retrieval::{retrieve_https, retrieve_source};
pub use url::{CanonicalUrl, classify_url};
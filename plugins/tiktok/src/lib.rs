mod url;

// `error` is scaffolded per design T3.5 with the `ResolverError` helpers
// (InvalidInput, MalformedResponse, UnsupportedUrl, transport/status maps)
// that Slice 2's retrieval/payload/mapping call. No Slice 1 path exercises
// them yet, so `dead_code` is suppressed at the module boundary; Slice 2
// removes this attribute once retrieval.rs calls `invalid_input`/`transport`/
// `status`/`unsupported`/`malformed`.
#[allow(dead_code)]
mod error;

pub use url::{CanonicalUrl, classify_url};
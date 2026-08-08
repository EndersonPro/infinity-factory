//! Public X/Twitter post resolver, via the syndication endpoint.
//!
//! One anonymous GET to `cdn.syndication.twimg.com/tweet-result` returns the
//! post as JSON, including progressive MP4 renditions on `video.twimg.com`.
//! No login, no guest token, no bearer constant, no JavaScript, no manifest.
//!
//! The API path yt-dlp prefers is deliberately not implemented: it needs an
//! `Authorization` header and a generic POST, neither of which the v2 host
//! contract expresses, and widening that contract to reach one site would cost
//! more than the site is worth. See
//! `openspec/changes/add-x-syndication-resolver/explore.md`.
//!
//! Behaviour is reimplemented from observation of the public endpoint and from
//! the ECMAScript specification for the token; yt-dlp
//! (`acf8ab7a6e3024325f62426e35a17f365c4d5d54`,
//! `yt_dlp/extractor/twitter.py:1150-1174`) is behavioural reference and test
//! oracle only.

mod error;
mod mapping;
mod payload;
mod retrieval;
mod token;
pub mod url;

pub use payload::{Selection, Variant};
pub use retrieval::{request_url, resolve_public};
pub use token::syndication_token;
pub use url::{PostId, classify_url};

use bex_media_url_resolver_v2::{ResolveResponse, ResolverError};

/// The pure half: a syndication body in, a typed response out, no host.
///
/// Split from [`resolve_public`] so every parsing and mapping decision is
/// testable against a committed fixture without a mock in the way.
pub fn parse_and_map(body: &[u8]) -> Result<ResolveResponse, ResolverError> {
    match payload::parse(body)? {
        Some(selection) => Ok(mapping::map(selection)),
        None => Ok(mapping::nothing_to_resolve()),
    }
}

#[cfg(target_arch = "wasm32")]
pub struct Component;

#[cfg(target_arch = "wasm32")]
impl bex_media_url_resolver_v2::ResolverGuest for Component {
    fn resolve(
        request: bex_media_url_resolver_v2::ResolveRequest,
    ) -> Result<bex_media_url_resolver_v2::ResolveResponse, bex_media_url_resolver_v2::ResolverError>
    {
        let mut client = bex_media_url_resolver_v2::WasmHttpsClient;
        let response = resolve_public(&mut client, &request.source_url)?;
        bex_media_url_resolver_v2::validate_resolver_response(&response)?;
        Ok(response)
    }
}

#[cfg(target_arch = "wasm32")]
bex_media_url_resolver_v2::export_resolver_v2!(Component);

use crate::{mapping, payload, retrieve_public};
use bex_media_url_resolver_v2::{HttpsClient, ResolveResponse, ResolverError, ResolverErrorKind};

pub fn resolve_public(
    client: &mut impl HttpsClient,
    source: &str,
) -> Result<ResolveResponse, ResolverError> {
    let retrieved = retrieve_public(client, source)?;
    match payload::graphql(retrieved.graphql_body()) {
        Ok(payload) => Ok(mapping::map(payload)),
        Err(original) if original.kind == ResolverErrorKind::MalformedResponse => {
            match retrieved.relay_json() {
                Some(relay) => payload::relay(relay).map(mapping::map),
                None => Err(original),
            }
        }
        Err(original) => Err(original),
    }
}

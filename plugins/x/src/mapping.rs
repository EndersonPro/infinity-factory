use crate::payload::{Selection, Variant};
use bex_media_url_resolver_v2::{
    Candidate, MediaStream, Metadata, Resolution, ResolveResponse, Unsupported,
};

fn stream(variant: &Variant) -> MediaStream {
    MediaStream {
        url: variant.url.clone(),
        format: Some("mp4".to_owned()),
        mime_type: Some("video/mp4".to_owned()),
        quality_label: variant.quality_label.clone(),
        codecs: None,
        // The rendition URLs carry no expiry marker, so none is claimed. An
        // invented one would have the app discard a file that still plays.
        expires_at_unix_seconds: None,
        byte_range_supported: false,
        headers: vec![],
    }
}

pub fn map(selection: Selection) -> ResolveResponse {
    let metadata = Some(Metadata {
        title: selection.title,
        author: selection.author,
        thumbnail_url: selection.thumbnail,
        duration_milliseconds: selection.duration_ms,
    });
    let resolution = match selection.variants.len() {
        1 => Resolution::Direct(stream(&selection.variants[0])),
        _ => Resolution::Candidates(
            selection
                .variants
                .iter()
                .map(|variant| Candidate {
                    id: variant.id.clone(),
                    stream: stream(variant),
                })
                .collect(),
        ),
    };
    ResolveResponse {
        metadata,
        resolution,
    }
}

/// A post that exists and carries no native video.
///
/// `unsupported` rather than an error, because nothing went wrong: the link
/// was valid, the fetch succeeded, and the answer is that there is nothing to
/// download. Roughly two in five public posts are this.
pub fn nothing_to_resolve() -> ResolveResponse {
    ResolveResponse {
        metadata: None,
        resolution: Resolution::Unsupported(Unsupported {
            reason: "this X post has no downloadable video".to_owned(),
        }),
    }
}

use crate::payload::Media;
use bex_media_url_resolver_v2::{
    Candidate, MediaStream, Metadata, Resolution, ResolveResponse, Unsupported,
};

fn stream(url: String) -> MediaStream {
    MediaStream {
        url,
        format: Some("mp4".into()),
        mime_type: Some("video/mp4".into()),
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: None,
        byte_range_supported: false,
        headers: vec![],
    }
}

/// Map a `Media` with surviving progressive URLs to `direct` (one) or
/// `candidates` (many), attaching bounded metadata (spec Req 4 / Req 8).
/// Metadata is omitted for an `Unsupported` outcome (empty progressive).
pub fn map_progressive(media: &Media) -> ResolveResponse {
    let metadata = Some(Metadata {
        title: media.title.clone(),
        author: media.author.clone(),
        thumbnail_url: media.thumbnail.clone(),
        duration_milliseconds: media.duration_milliseconds,
    });
    let resolution = if media.progressive.len() == 1 {
        Resolution::Direct(stream(media.progressive[0].clone()))
    } else if !media.progressive.is_empty() {
        Resolution::Candidates(
            media
                .progressive
                .iter()
                .enumerate()
                .map(|(index, url)| Candidate {
                    id: format!("media-{index:04}"),
                    stream: stream(url.clone()),
                })
                .collect(),
        )
    } else {
        Resolution::Unsupported(Unsupported {
            reason: "no supported Facebook progressive URL".into(),
        })
    };
    let metadata = if matches!(resolution, Resolution::Unsupported(_)) {
        None
    } else {
        metadata
    };
    ResolveResponse {
        metadata,
        resolution,
    }
}

/// Build an `Unsupported` response with no metadata (spec Req 7 / Req 8).
pub fn unsupported_response(reason: &str) -> ResolveResponse {
    ResolveResponse {
        metadata: None,
        resolution: Resolution::Unsupported(Unsupported {
            reason: reason.into(),
        }),
    }
}

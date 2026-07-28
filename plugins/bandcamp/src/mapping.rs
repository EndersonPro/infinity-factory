use crate::payload::{RetainedFormat, Selection};
use bex_media_url_resolver_v2::{Candidate, MediaStream, Metadata, Resolution, ResolveResponse};

fn mime_for(ext: &str) -> Option<&'static str> {
    match ext {
        "mp3" => Some("audio/mpeg"),
        "m4a" => Some("audio/mp4"),
        "ogg" => Some("audio/ogg"),
        "flac" => Some("audio/flac"),
        "wav" => Some("audio/wav"),
        "opus" => Some("audio/opus"),
        _ => None,
    }
}

fn stream(format: &RetainedFormat) -> MediaStream {
    let ext = format
        .format_id
        .split('-')
        .next()
        .unwrap_or(&format.format_id);
    MediaStream {
        url: format.url.clone(),
        format: Some(ext.to_owned()),
        mime_type: mime_for(ext).map(str::to_owned),
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: format.expires_at,
        byte_range_supported: false,
        headers: vec![],
    }
}

pub fn map(selection: Selection) -> ResolveResponse {
    let metadata = Some(Metadata {
        title: selection.title,
        author: selection.artist,
        thumbnail_url: selection.thumbnail,
        duration_milliseconds: selection.duration_ms,
    });
    let resolution = match selection.formats.len() {
        1 => Resolution::Direct(stream(&selection.formats[0])),
        _ => Resolution::Candidates(
            selection
                .formats
                .iter()
                .map(|format| Candidate {
                    id: format.format_id.clone(),
                    stream: stream(format),
                })
                .collect(),
        ),
    };
    ResolveResponse {
        metadata,
        resolution,
    }
}

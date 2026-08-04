use crate::payload::{Finalized, ResolvedFormat};
use bex_media_url_resolver_v2::{
    Candidate, MediaStream, Metadata, MuxContainer, MuxPlan, Resolution, ResolveResponse,
    SeparatedStreams, Unsupported,
};

/// Split a YouTube `mimeType` such as `video/mp4; codecs="avc1.4d401f, mp4a.40.2"`
/// into a short `format` (the subtype, e.g. `"mp4"`) and the `codecs` string.
fn mime_parts(mime_type: &str) -> (Option<String>, Option<String>) {
    let (family, rest) = mime_type
        .split_once(';')
        .map_or((mime_type, ""), |(family, rest)| (family, rest));
    let format = family
        .split_once('/')
        .map(|(_, subtype)| subtype.to_owned());
    let codecs = rest.split_once("codecs=").and_then(|(_, value)| {
        let value = value.trim().trim_matches('"');
        (!value.is_empty()).then(|| value.to_owned())
    });
    (format, codecs)
}

fn short_mime(mime_type: &str) -> String {
    mime_type
        .split_once(';')
        .map_or(mime_type, |(family, _)| family)
        .trim()
        .to_owned()
}

/// YouTube CDN stream URLs carry their expiry as an `expire=<unix-seconds>`
/// query parameter (present both on direct and cipher-decoded URLs).
fn expires_at(url: &str) -> Option<u64> {
    url.split(['?', '&'])
        .find_map(|pair| pair.strip_prefix("expire="))
        .and_then(|value| value.parse().ok())
}

fn stream(format: &ResolvedFormat) -> MediaStream {
    let (format_ext, codecs) = mime_parts(&format.mime_type);
    MediaStream {
        url: format.url.clone(),
        format: format_ext,
        mime_type: Some(short_mime(&format.mime_type)),
        quality_label: format.quality_label.clone(),
        codecs,
        expires_at_unix_seconds: expires_at(&format.url),
        // YouTube's googlevideo.com CDN is documented to support HTTP Range
        // requests on every stream URL it issues (required for seeking); this
        // is a well-known, stable CDN property, not a per-URL guess.
        byte_range_supported: true,
        headers: vec![],
    }
}

/// Pick a mux container compatible with both codec families without a
/// re-encode: same-family pairs keep their native container (mp4+mp4 ->
/// mp4, webm+webm -> webm); a mixed pair (e.g. vp9/webm video with mp4a/mp4
/// audio) needs Matroska, which can hold either codec family without
/// transcoding.
fn container_for(video_mime: &str, audio_mime: &str) -> MuxContainer {
    if video_mime.starts_with("video/mp4") && audio_mime.starts_with("audio/mp4") {
        MuxContainer::Mp4
    } else if video_mime.starts_with("video/webm") && audio_mime.starts_with("audio/webm") {
        MuxContainer::Webm
    } else {
        MuxContainer::Matroska
    }
}

pub(crate) fn map(finalized: Finalized) -> ResolveResponse {
    let metadata = Some(Metadata {
        title: finalized.title,
        author: finalized.author,
        thumbnail_url: finalized.thumbnail,
        duration_milliseconds: finalized.duration_ms,
    });
    let resolution = if !finalized.progressive.is_empty() {
        if finalized.progressive.len() == 1 {
            Resolution::Direct(stream(&finalized.progressive[0]))
        } else {
            Resolution::Candidates(
                finalized
                    .progressive
                    .iter()
                    .enumerate()
                    .map(|(index, format)| Candidate {
                        id: format!("progressive-{index:04}"),
                        stream: stream(format),
                    })
                    .collect(),
            )
        }
    } else {
        match (finalized.audio, finalized.video) {
            (Some(audio), Some(video)) => {
                let container = container_for(&video.mime_type, &audio.mime_type);
                Resolution::Separated(SeparatedStreams {
                    audio: stream(&audio),
                    video: stream(&video),
                    mux_plan: MuxPlan {
                        container,
                        prefer_stream_copy: true,
                    },
                })
            }
            (Some(audio), None) => Resolution::Direct(stream(&audio)),
            (None, Some(video)) => Resolution::Direct(stream(&video)),
            (None, None) => Resolution::Unsupported(Unsupported {
                reason: "no supported YouTube stream formats were available".into(),
            }),
        }
    };
    ResolveResponse {
        metadata,
        resolution,
    }
}

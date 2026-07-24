use crate::payload::{Node, Payload};
use bex_media_url_resolver_v2::{
    Candidate, MediaStream, Metadata, Resolution, ResolveResponse, Unsupported,
};

fn stream(node: Node) -> MediaStream {
    let (url, format, mime_type) = match node {
        Node::Video { url } => (url, Some("mp4".into()), Some("video/mp4".into())),
        Node::Image { url } => (url, None, None),
    };
    MediaStream {
        url,
        format,
        mime_type,
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: None,
        byte_range_supported: false,
        headers: vec![],
    }
}
pub(crate) fn map(payload: Payload) -> ResolveResponse {
    let metadata = Some(Metadata {
        title: payload.title,
        author: payload.author,
        thumbnail_url: Some(payload.thumbnail),
        duration_milliseconds: None,
    });
    let resolution = match payload.children {
        Some(children) => Resolution::Candidates(
            children
                .into_iter()
                .enumerate()
                .map(|(index, node)| Candidate {
                    id: format!("media-{index:04}"),
                    stream: stream(node),
                })
                .collect(),
        ),
        None => match payload.root {
            Node::Video { url } => Resolution::Direct(stream(Node::Video { url })),
            Node::Image { .. } => Resolution::Unsupported(Unsupported {
                reason: "public Instagram item contains no supported video".into(),
            }),
        },
    };
    ResolveResponse {
        metadata,
        resolution,
    }
}

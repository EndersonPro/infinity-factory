use bandcamp::parse_and_map;
use bex_media_url_resolver_v2::{Resolution, ResolverErrorKind};

fn encode(json: &str) -> String {
    json.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&#39;")
        .replace('"', "&quot;")
}

fn page_inner(tralbum: &str, og: Option<&str>) -> String {
    let og_tag = match og {
        Some(url) => format!(r#"<meta property="og:image" content="{url}">"#),
        None => String::new(),
    };
    format!(
        "<html><head>{og_tag}</head><body><div data-tralbum=\"{}\"></div></body></html>",
        encode(tralbum)
    )
}

fn page(tralbum: &str, og: Option<&str>) -> Vec<u8> {
    page_inner(tralbum, og).into_bytes()
}

const DIRECT_JSON: &str = r#"{"current":{"id":100,"title":"Song","artist":"Artist"},"trackinfo":[{"track_id":100,"title":"Song","duration":9.5,"file":{"mp3-128":"https://t4.bcbits.com/stream/abc?ts=1000000&token=1000000xyz"}}]}"#;
const OG: &str = "https://f4.bcbits.com/img/a1_5.jpg";

#[test]
fn maps_one_format_as_direct_with_metadata_and_expiry() {
    let response = parse_and_map(&page(DIRECT_JSON, Some(OG)), 500_000).unwrap();
    let meta = response.metadata.unwrap();
    assert_eq!(meta.title.as_deref(), Some("Song"));
    assert_eq!(meta.author.as_deref(), Some("Artist"));
    assert_eq!(meta.thumbnail_url.as_deref(), Some(OG));
    assert_eq!(meta.duration_milliseconds, Some(9500));
    match response.resolution {
        Resolution::Direct(stream) => {
            assert_eq!(
                stream.url,
                "https://t4.bcbits.com/stream/abc?ts=1000000&token=1000000xyz"
            );
            assert_eq!(stream.format.as_deref(), Some("mp3"));
            assert_eq!(stream.mime_type.as_deref(), Some("audio/mpeg"));
            assert_eq!(stream.expires_at_unix_seconds, Some(1_000_000));
            assert!(stream.headers.is_empty());
            assert!(!stream.byte_range_supported);
        }
        other => panic!("expected direct, got {other:?}"),
    }
}

#[test]
fn maps_multiple_formats_as_ordered_candidates_with_unique_ids() {
    let json = r#"{"current":{"id":1,"title":"T","artist":"A"},"trackinfo":[{"track_id":1,"title":"T","duration":0,"file":{"mp3-128":"https://t4.bcbits.com/a?ts=1000000&token=1000000a","mp3-320":"https://t4.bcbits.com/b?ts=1000000&token=1000000b","m4a-128":"https://t4.bcbits.com/c?ts=1000000&token=1000000c"}}]}"#;
    let response = parse_and_map(&page(json, None), 0).unwrap();
    assert_eq!(response.metadata.unwrap().duration_milliseconds, Some(0));
    match response.resolution {
        Resolution::Candidates(items) => {
            assert_eq!(items.len(), 3);
            assert_eq!(items[0].id, "mp3-128");
            assert_eq!(items[1].id, "mp3-320");
            assert_eq!(items[2].id, "m4a-128");
            assert_eq!(items[2].stream.mime_type.as_deref(), Some("audio/mp4"));
        }
        other => panic!("expected candidates, got {other:?}"),
    }
}

#[test]
fn deduplicates_repeated_urls_in_source_order() {
    let json = r#"{"current":{"id":1,"title":"T"},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a","mp3-320":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}]}"#;
    let response = parse_and_map(&page(json, None), 0).unwrap();
    match response.resolution {
        Resolution::Direct(stream) => assert_eq!(stream.format.as_deref(), Some("mp3")),
        other => panic!("expected direct after dedupe, got {other:?}"),
    }
}

#[test]
fn drops_expired_and_returns_unavailable_when_only_expired_or_empty() {
    assert_eq!(
        parse_and_map(&page(DIRECT_JSON, None), 1_000_000)
            .unwrap_err()
            .kind,
        ResolverErrorKind::Unavailable
    );
    let empty = r#"{"current":{"id":100},"trackinfo":[{"track_id":100,"title":"T","file":{"mp3-128":""}}]}"#;
    assert_eq!(
        parse_and_map(&page(empty, None), 0).unwrap_err().kind,
        ResolverErrorKind::Unavailable
    );
    let no_trackinfo = r#"{"current":{"id":100},"trackinfo":[]}"#;
    assert_eq!(
        parse_and_map(&page(no_trackinfo, None), 0)
            .unwrap_err()
            .kind,
        ResolverErrorKind::Unavailable
    );
}

#[test]
fn rejects_unsafe_populated_media_urls_as_malformed() {
    for bad in [
        r#"{"current":{"id":1},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"http://t4.bcbits.com/x?ts=1&token=1a"}}]}"#,
        r#"{"current":{"id":1},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1&token=1a#frag"}}]}"#,
    ] {
        assert_eq!(
            parse_and_map(&page(bad, None), 0).unwrap_err().kind,
            ResolverErrorKind::MalformedResponse,
            "accepted {bad}"
        );
    }
}

#[test]
fn rejects_absent_ambiguous_or_conflicting_current_selection() {
    for (label, json) in [
        (
            "no current",
            r#"{"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}]}"#,
        ),
        (
            "current no id",
            r#"{"current":{"title":"T"},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}]}"#,
        ),
        (
            "no match",
            r#"{"current":{"id":2},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}]}"#,
        ),
        (
            "ambiguous match",
            r#"{"current":{"id":1},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}},{"id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/y?ts=1000000&token=1000000b"}}]}"#,
        ),
    ] {
        assert_eq!(
            parse_and_map(&page(json, None), 0).unwrap_err().kind,
            ResolverErrorKind::MalformedResponse,
            "accepted {label}"
        );
    }
}

#[test]
fn rejects_conflicting_expiry_markers_as_malformed() {
    for (label, url) in [
        ("ts without token", "https://t4.bcbits.com/x?ts=1000000"),
        (
            "ts token mismatch",
            "https://t4.bcbits.com/x?ts=1000000&token=9999abc",
        ),
        (
            "non-decimal ts",
            "https://t4.bcbits.com/x?ts=soon&token=soonabc",
        ),
        (
            "duplicate ts",
            "https://t4.bcbits.com/x?ts=1&ts=2&token=1abc",
        ),
        (
            "fragment in media url",
            "https://t4.bcbits.com/x?ts=1000000&token=1000000a#frag",
        ),
    ] {
        let json = format!(
            r#"{{"current":{{"id":1}},"trackinfo":[{{"track_id":1,"title":"T","file":{{"mp3-128":"{url}"}}}}]}}"#
        );
        assert_eq!(
            parse_and_map(&page(&json, None), 0).unwrap_err().kind,
            ResolverErrorKind::MalformedResponse,
            "accepted {label}"
        );
    }
}

#[test]
fn accepts_absent_expiry_as_none() {
    let json = r#"{"current":{"id":1},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?token=abc"}}]}"#;
    let response = parse_and_map(&page(json, None), 0).unwrap();
    match response.resolution {
        Resolution::Direct(stream) => assert_eq!(stream.expires_at_unix_seconds, None),
        other => panic!("expected direct, got {other:?}"),
    }
}

#[test]
fn enforces_html_decode_and_structure_bounds() {
    assert_eq!(
        parse_and_map(&[0xff, 0xfe], 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
    assert_eq!(
        parse_and_map(b"<html></html>", 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
    let two = page_inner(DIRECT_JSON, None) + &page_inner(DIRECT_JSON, None);
    assert_eq!(
        parse_and_map(two.as_bytes(), 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
    let mut raw = encode(r#"{"current":{"id":1},"trackinfo":[]}"#);
    raw.push_str("&unknown;");
    let bad_entity = format!("<html><body><div data-tralbum=\"{raw}\"></div></body></html>");
    assert_eq!(
        parse_and_map(bad_entity.as_bytes(), 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
    // body > 4 MiB
    let huge = vec![b'a'; 4_194_305];
    assert_eq!(
        parse_and_map(&huge, 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn decodes_named_and_numeric_entities() {
    let json = r#"{"current":{"id":1,"title":"a'b<c>d&e"},"trackinfo":[{"track_id":1,"title":"a'b<c>d&e","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}]}"#;
    let response = parse_and_map(&page(json, None), 0).unwrap();
    assert_eq!(
        response.metadata.unwrap().title.as_deref(),
        Some("a'b<c>d&e")
    );
}

#[test]
fn enforces_json_depth_members_strings_trackinfo_and_file_caps() {
    fn deep_json(depth: usize) -> String {
        let prefix = r#"{"current":{"id":1},"trackinfo":[{"track_id":1,"title":"T","file":{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}],"nested":"#;
        let mut s = String::from(prefix);
        for _ in 0..depth {
            s.push_str("{\"nested\":");
        }
        s.push_str("{\"x\":1}");
        for _ in 0..depth {
            s.push('}');
        }
        s.push('}');
        s
    }
    assert!(parse_and_map(&page(&deep_json(30), None), 0).is_ok());
    assert_eq!(
        parse_and_map(&page(&deep_json(32), None), 0)
            .unwrap_err()
            .kind,
        ResolverErrorKind::MalformedResponse
    );
    let long_string = format!(
        r#"{{"current":{{"id":1}},"trackinfo":[{{"track_id":1,"title":"{}","file":{{"mp3-128":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}}}]}}"#,
        "x".repeat(4097)
    );
    assert_eq!(
        parse_and_map(&page(&long_string, None), 0)
            .unwrap_err()
            .kind,
        ResolverErrorKind::MalformedResponse
    );
    let many_trackinfo: String = (0..17)
        .map(|i| format!(r#"{{"track_id":999,"title":"T","file":{{"mp3-{i}":"https://t4.bcbits.com/x?ts=1000000&token=1000000a"}}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let over_trackinfo = format!(r#"{{"current":{{"id":1}},"trackinfo":[{many_trackinfo}]}}"#);
    assert_eq!(
        parse_and_map(&page(&over_trackinfo, None), 0)
            .unwrap_err()
            .kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn caps_candidates_at_sixteen_and_rejects_seventeen() {
    let mut entries = String::new();
    for i in 0..16 {
        if !entries.is_empty() {
            entries.push(',');
        }
        entries.push_str(&format!(
            r#""mp3-{i:02}":"https://t4.bcbits.com/u{i}?ts=1000000&token=1000000a""#
        ));
    }
    let ok = format!(
        r#"{{"current":{{"id":1}},"trackinfo":[{{"track_id":1,"title":"T","file":{{{entries}}}}}]}}"#
    );
    match parse_and_map(&page(&ok, None), 0).unwrap().resolution {
        Resolution::Candidates(items) => assert_eq!(items.len(), 16),
        other => panic!("expected 16 candidates, got {other:?}"),
    }
    let seventeen = format!(
        r#"{{"current":{{"id":1}},"trackinfo":[{{"track_id":1,"title":"T","file":{{{entries},"mp3-99":"https://t4.bcbits.com/u99?ts=1000000&token=1000000a"}}}}]}}"#
    );
    assert_eq!(
        parse_and_map(&page(&seventeen, None), 0).unwrap_err().kind,
        ResolverErrorKind::MalformedResponse
    );
}

#[test]
fn omits_unsafe_or_duplicate_thumbnail() {
    let page_html = page(DIRECT_JSON, Some("http://evil.example/x.jpg"));
    assert_eq!(
        parse_and_map(&page_html, 0)
            .unwrap()
            .metadata
            .unwrap()
            .thumbnail_url,
        None
    );
    let two_og = format!(
        r#"<html><head><meta property="og:image" content="{OG}"><meta property="og:image" content="{OG}"></head><body><div data-tralbum="{}"></div></body></html>"#,
        encode(DIRECT_JSON)
    );
    assert_eq!(
        parse_and_map(two_og.as_bytes(), 0)
            .unwrap()
            .metadata
            .unwrap()
            .thumbnail_url,
        None
    );
}

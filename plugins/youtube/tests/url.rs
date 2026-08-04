use youtube::classify_url;

const ID: &str = "dQw4w9WgXcQ";

#[test]
fn classifies_supported_watch_shorts_and_short_link_shapes() {
    for (input, expected_id) in [
        (format!("https://www.youtube.com/watch?v={ID}"), ID),
        (format!("https://youtube.com/watch?v={ID}"), ID),
        (format!("https://m.youtube.com/watch?v={ID}"), ID),
        (format!("https://www.youtube.com/shorts/{ID}"), ID),
        (format!("https://www.youtube.com/shorts/{ID}/"), ID),
        (format!("https://youtu.be/{ID}"), ID),
    ] {
        let canonical =
            classify_url(&input).unwrap_or_else(|e| panic!("rejected {input}: {e}"));
        assert_eq!(canonical.as_str(), expected_id, "input: {input}");
        assert_eq!(
            canonical.watch_url(),
            format!("https://www.youtube.com/watch?v={expected_id}")
        );
    }
}

#[test]
fn rejects_unsupported_or_malformed_sources_before_host_use() {
    for input in [
        "",
        "http://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "HTTPS://www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com/watch",
        "https://www.youtube.com/watch?v=short",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQextra",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=xyz",
        "https://www.youtube.com/watch?list=xyz&v=dQw4w9WgXcQ",
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ#frag",
        "https://user@www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com:443/watch?v=dQw4w9WgXcQ",
        "https://evil.example/watch?v=dQw4w9WgXcQ",
        "https://youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com/embed/dQw4w9WgXcQ",
        "https://www.youtube.com/v/dQw4w9WgXcQ",
        "https://www.youtube.com/shorts/dQw4w9WgXcQ?feature=share",
        "https://youtu.be/dQw4w9WgXcQ?t=10",
        "https://youtu.be/dQw4w9WgXcQ/extra",
        "https://youtu.be/short",
        "www.youtube.com/watch?v=dQw4w9WgXcQ",
        "https://www.youtube.com/watch?v=dQw4w9WgXc!",
    ] {
        assert!(classify_url(input).is_err(), "accepted {input}");
    }
}

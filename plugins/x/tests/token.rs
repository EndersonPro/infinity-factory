//! Cross-check of the ES5 9.8.1 reimplementation.
//!
//! The expected values were produced by yt-dlp's own implementation of the
//! same specification at commit `acf8ab7a6e3024325f62426e35a17f365c4d5d54`
//! (`yt_dlp/jsinterp.py:107`). Using its output as an oracle is the intended
//! use of a public-domain reference; no code was transplanted.

use x::syndication_token;

#[test]
fn matches_the_reference_oracle() {
    for (post_id, expected) in [
        (1163218564784017422u64, "2ticx4q85hf"),
        (1047395834013384704, "2jehoes3gic"),
        (1348948114569269251, "39pufr15xm4"),
        (1001551623938805763, "2fegtisuq5"),
        (1577719286659006464, "3tojuiek1l"),
    ] {
        assert_eq!(syndication_token(post_id), expected, "{post_id}");
    }
}

/// The derivation strips `0` and `.`, so neither can appear. The transport
/// allowlist relies on that: it admits `1-9a-z` and refuses a token carrying a
/// zero, which would be a string this function cannot produce.
#[test]
fn never_emits_a_zero_or_a_point() {
    for post_id in [1u64, 20, 999, 1_000_000_000_000, 1_755_000_000_000_000_000] {
        let token = syndication_token(post_id);
        assert!(!token.is_empty(), "{post_id}");
        assert!(token.len() <= 16, "{post_id}: {token}");
        assert!(
            token
                .bytes()
                .all(|value| matches!(value, b'1'..=b'9') || value.is_ascii_lowercase()),
            "{post_id}: {token}"
        );
    }
}

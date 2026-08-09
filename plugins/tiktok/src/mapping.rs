use crate::payload::VideoData;
use bex_media_url_resolver_v2::{resolver_bounds, Candidate, MediaStream, Resolution, Unsupported};
use url::Url;

/// Suffix-exact, non-apex admitted output hosts (design.md:19). Each arm
/// matches a literal suffix AND rejects the bare apex of that suffix; no
/// wildcards — `compatibility/v2/abi-identity-vectors.json` names
/// `wildcard-host` and `deceptive-host` as invalid ABI shapes.
fn admitted_output_family(host: &str) -> bool {
    let cdn = host.ends_with(".tiktokcdn.com") && host != "tiktokcdn.com";
    let tiktokv = host.ends_with(".tiktokv.com") && host != "tiktokv.com";
    let webapp_prime = host.starts_with('v')
        && host.ends_with("-webapp-prime.tiktok.com")
        && host != "webapp-prime.tiktok.com";
    cdn || tiktokv || webapp_prime
}

/// Three exact, non-apex CDN family checks. Each is a disjunctive arm: the
/// host must MATCH a literal suffix AND must not be the bare apex of that
/// suffix. No wildcards — `compatibility/v2/abi-identity-vectors.json` names
/// `wildcard-host` and `deceptive-host` as invalid ABI shapes.
///
/// 1. `host.ends_with(".tiktokcdn.com")` and `host != "tiktokcdn.com"`
/// 2. `host.ends_with(".tiktokv.com")` and `host != "tiktokv.com"`
/// 3. `host.starts_with('v')` and `host.ends_with("-webapp-prime.tiktok.com")`
///    and `host != "webapp-prime.tiktok.com"` (rejects the no-`v` apex)
///
/// HTTPS only; userinfo, explicit port, and fragment are all rejected. The
/// SDK's `valid_headers` rejects `cookie`/`authorization` response headers
/// separately; this predicate does not duplicate that check (design.md:19).
pub fn is_safe_tiktok_output_url(u: &Url) -> bool {
    let Some(host) = u.host_str() else {
        return false;
    };
    if u.scheme() != "https"
        || !u.username().is_empty()
        || u.password().is_some()
        || u.port().is_some()
        || u.fragment().is_some()
        || u.as_str().len() > resolver_bounds::URL
    {
        return false;
    }
    admitted_output_family(host)
}

/// The TikTok internal player redirect; declared out of output scope
/// (`proposal.md:165-168`). `www.tiktok.com/aweme/v1/play/...` is a redirect,
/// not a direct CDN URL, so candidates are filtered by `!is_gateway_url(u)`.
pub fn is_gateway_url(u: &Url) -> bool {
    u.host_str() == Some("www.tiktok.com") && u.path().starts_with("/aweme/v1/play/")
}

fn stream(url: &str) -> MediaStream {
    MediaStream {
        url: url.to_owned(),
        format: Some("mp4".to_owned()),
        mime_type: Some("video/mp4".to_owned()),
        quality_label: None,
        codecs: None,
        expires_at_unix_seconds: None,
        byte_range_supported: false,
        headers: vec![],
    }
}

/// Spec Req 5 + Req 6: collect candidate HTTPS MP4 URLs from `playAddr`, the
/// optional `downloadAddr`, `PlayAddrStruct.UrlList`, and
/// `bitrateInfo[*].PlayAddr.UrlList`, in source order; keep only
/// `!is_gateway_url ∧ is_safe_tiktok_output_url` URLs; deduplicate by URL
/// value preserving source order; cap at 16 (the SDK bound). 1 survivor →
/// `Direct`; ≥2 → `Candidates`; 0 → `Unsupported`. `downloadAddr` is optional;
/// its absence never produces `Unsupported` when `playAddr` is present
/// (spec Req 6 download-disabled scenario).
pub fn map(video: &VideoData) -> Resolution {
    let mut raw: Vec<&str> = Vec::new();
    if let Some(play_addr) = video.play_addr.as_deref() {
        raw.push(play_addr);
    }
    if let Some(download_addr) = video.download_addr.as_deref() {
        raw.push(download_addr);
    }
    for url in &video.play_addr_struct_url_list {
        raw.push(url);
    }
    for url in &video.bitrate_info_url_lists {
        raw.push(url);
    }

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut survivors: Vec<String> = Vec::new();
    for url_str in raw {
        if survivors.len() >= resolver_bounds::CANDIDATES {
            break;
        }
        if url_str.len() > resolver_bounds::URL {
            continue;
        }
        let Ok(parsed) = Url::parse(url_str) else {
            continue;
        };
        if is_gateway_url(&parsed) || !is_safe_tiktok_output_url(&parsed) {
            continue;
        }
        if !seen.insert(url_str.to_owned()) {
            continue;
        }
        survivors.push(url_str.to_owned());
    }

    match survivors.len() {
        0 => Resolution::Unsupported(Unsupported {
            reason: "public TikTok video has no supported CDN URL".into(),
        }),
        1 => Resolution::Direct(stream(&survivors[0])),
        _ => {
            let candidates: Vec<Candidate> = survivors
                .iter()
                .enumerate()
                .map(|(index, url)| Candidate {
                    id: index.to_string(),
                    stream: stream(url),
                })
                .collect();
            Resolution::Candidates(candidates)
        }
    }
}
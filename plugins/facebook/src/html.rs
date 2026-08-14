/// Login-wall markers a logged-out Facebook page emits while still answering
/// HTTP 200 (spec Req 6). Detection is HTML-substring based because the status
/// code is always 200 on a login wall (`exploration.md:122`).
const LOGIN_MARKERS: [&str; 4] = [
    "class=\"uiInterstitialContent\"",
    "id=\"login_form\"",
    "id=\"loginbutton\"",
    ">You must log in to continue",
];

const SCRIPT_END: &str = "</script>";
const DATA_SJS_ATTR: &str = "data-sjs=\"";

/// True when any Facebook login-wall marker is present in the page HTML
/// (spec Req 6 scenarios). Detection precedes any progressive/Tahoe parsing.
pub fn is_login_wall(html: &str) -> bool {
    LOGIN_MARKERS.iter().any(|marker| html.contains(marker))
}

/// Extract the inner JSON text of every `<script ... data-sjs="..."> ... </script>`
/// block in the page (spec Req 3). The JSON is the script TEXT content, not the
/// attribute value; malformed JSON is surfaced as a `malformed-response` outcome
/// by the payload layer, not here.
pub fn extract_data_sjs(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search = 0usize;
    while let Some(relative) = html[search..].find(DATA_SJS_ATTR) {
        let tag_start = search + relative;
        let after_attr = &html[tag_start + DATA_SJS_ATTR.len()..];
        let Some(close) = after_attr.find('>') else {
            break;
        };
        let inner_start = tag_start + DATA_SJS_ATTR.len() + close + 1;
        let Some(end_rel) = html[inner_start..].find(SCRIPT_END) else {
            break;
        };
        let inner = &html[inner_start..inner_start + end_rel];
        blocks.push(inner.to_owned());
        search = inner_start + end_rel + SCRIPT_END.len();
    }
    blocks
}
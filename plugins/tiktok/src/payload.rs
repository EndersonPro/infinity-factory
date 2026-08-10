use crate::error;
use bex_media_url_resolver_v2::{ResolverError, bounds};

/// Script tag the resolver scans for. The response body is plain text; there is
/// no JavaScript execution, no entity decode, no DOM. The resolver locates
/// exactly this block and JSON-parses its contents.
const UNIVERSAL_OPEN: &str =
    "<script id=\"__UNIVERSAL_DATA_FOR_REHYDRATION__\" type=\"application/json\">";
const UNIVERSAL_CLOSE: &str = "</script>";
/// TikTok's mobile rendering path embeds the same public video detail object
/// in this later JSON island instead of in universal rehydration.  The host
/// chooses that rendering path with its own anonymous browser identity; this
/// remains a body-only parse and never executes page JavaScript.
const API_DATA_OPEN: &str = "<script id=\"api-data\" type=\"application/json\">";

/// Per-list defensive cap so a hostile or drifting body cannot trigger
/// unbounded allocation inside the guest. The SDK body cap
/// (`bounds::RESPONSE_BODY`, 4 MiB) already bounds the whole response; this
/// bounds the per-collection fan-out. The mapping layer's own 16-candidate
/// cap is the user-visible bound.
const URL_LIST_CAP: usize = 256;

/// Bounded selection of the populated `webapp.video-detail.itemInfo.itemStruct.
/// video` subtree (spec Req 3). `statusCode` is read from `webapp.video-detail`
/// directly — the committed `tt_v3.html` oracle places it there, not on
/// `itemInfo`; a non-zero value maps to `Unsupported` (spec Req 4). The fields
/// hold the raw collected URLs pre-filter; the mapping layer applies the
/// CDN-family + gateway + dedup + cap rules.
#[derive(Debug)]
pub struct VideoData {
    pub status_code: i64,
    pub play_addr: Option<String>,
    pub download_addr: Option<String>,
    pub play_addr_struct_url_list: Vec<String>,
    pub bitrate_info_url_lists: Vec<String>,
}

fn as_object<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Map<String, serde_json::Value>> {
    value.get(key).and_then(serde_json::Value::as_object)
}

fn collect_url_list(target: &mut Vec<String>, value: &serde_json::Value) {
    let Some(items) = value.as_array() else {
        return;
    };
    for item in items {
        if target.len() >= URL_LIST_CAP {
            break;
        }
        if let Some(text) = item.as_str()
            && !text.is_empty()
            && text.len() <= bounds::URL
        {
            target.push(text.to_owned());
        }
    }
}

fn bounded_string(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty() && text.len() <= bounds::URL)
        .map(str::to_owned)
}

/// Spec Req 3 + Req 4: locate the single
/// `<script id="__UNIVERSAL_DATA_FOR_REHYDRATION__" type="application/json">`
/// block, JSON-parse it, bind `__DEFAULT_SCOPE__.webapp.video-detail.itemInfo.
/// itemStruct.video`, gate on `statusCode == 0`. Missing block, missing
/// `webapp.video-detail`, missing `itemInfo.itemStruct.video`, malformed JSON,
/// and every non-zero `statusCode` all map to `Unsupported` (a content-state
/// outcome the host renders as `CaptureOutcome.empty`, never a download
/// failure). No itemModule defensive fallback: `itemInfo` is the binding
/// contract (spec Req 3 scenario "Returns Unsupported when
/// itemInfo.itemStruct.video is absent").
pub fn parse_universal_data(body: &[u8]) -> Result<VideoData, ResolverError> {
    let text = std::str::from_utf8(body).map_err(|_| error::unsupported())?;
    let universal_root = script_json(text, UNIVERSAL_OPEN);
    let api_root = script_json(text, API_DATA_OPEN);
    let video_detail = universal_root
        .as_ref()
        .and_then(|root| as_object(root, "__DEFAULT_SCOPE__"))
        .and_then(|scope| scope.get("webapp.video-detail"))
        .and_then(serde_json::Value::as_object)
        .or_else(|| {
            api_root
                .as_ref()
                .and_then(|root| root.get("videoDetail"))
                .and_then(serde_json::Value::as_object)
        })
        .ok_or_else(error::unsupported)?;
    let status_code = video_detail
        .get("statusCode")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(error::unsupported)?;
    if status_code != 0 {
        return Err(error::unsupported());
    }
    let item_info = video_detail
        .get("itemInfo")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(error::unsupported)?;
    let item_struct = item_info
        .get("itemStruct")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(error::unsupported)?;
    let video = item_struct
        .get("video")
        .filter(|value| value.is_object())
        .ok_or_else(error::unsupported)?;
    let video_obj = video.as_object().expect("video verified as object");

    let play_addr = video_obj.get("playAddr").and_then(bounded_string);
    let download_addr = video_obj.get("downloadAddr").and_then(bounded_string);

    let mut play_addr_struct_url_list = Vec::new();
    if let Some(play_addr_struct) = video_obj
        .get("PlayAddrStruct")
        .and_then(serde_json::Value::as_object)
        && let Some(url_list) = play_addr_struct.get("UrlList")
    {
        collect_url_list(&mut play_addr_struct_url_list, url_list);
    }

    let mut bitrate_info_url_lists = Vec::new();
    if let Some(bitrate_info) = video_obj
        .get("bitrateInfo")
        .and_then(serde_json::Value::as_array)
    {
        for entry in bitrate_info {
            if let Some(play_addr) = entry.get("PlayAddr").and_then(serde_json::Value::as_object)
                && let Some(url_list) = play_addr.get("UrlList")
            {
                collect_url_list(&mut bitrate_info_url_lists, url_list);
            }
        }
    }

    let any_present = play_addr.is_some()
        || download_addr.is_some()
        || !play_addr_struct_url_list.is_empty()
        || !bitrate_info_url_lists.is_empty();
    if !any_present {
        return Err(error::unsupported());
    }

    Ok(VideoData {
        status_code,
        play_addr,
        download_addr,
        play_addr_struct_url_list,
        bitrate_info_url_lists,
    })
}

/// Parses one explicitly named JSON script island.  Its bounded enclosing HTML
/// is capped by the SDK before this point; a missing or malformed island is a
/// content-state result rather than an upstream parser error.
fn script_json(text: &str, open_marker: &str) -> Option<serde_json::Value> {
    let open = text.find(open_marker)?;
    let json_start = open + open_marker.len();
    let close = text[json_start..].find(UNIVERSAL_CLOSE)?;
    serde_json::from_str(&text[json_start..json_start + close]).ok()
}

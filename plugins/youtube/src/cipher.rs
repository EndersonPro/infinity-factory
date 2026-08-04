//! Signature-cipher decoding for YouTube stream URLs.
//!
//! Ported (stream-resolution slice only) from the repo owner's own
//! `bloom-factory/ytvideo` crate's `src/cipher.rs`. That crate fetches
//! streams via YouTube's InnerTube `player` POST endpoint using several
//! device-client identities (ANDROID_VR/IOS/TVHTML5); this v2 resolver ABI
//! exposes only `get` (allowlisted per-host/path) and a GraphQL-shaped
//! `post-public-graphql` fixed to Instagram's endpoint — no generic POST is
//! available, so the InnerTube client strategy cannot be reused here. This
//! module keeps exactly the pure, host-independent text-parsing logic (the
//! player-JS cipher-operation extraction and the signature-decode algebra),
//! which is unchanged by how the player JS text was obtained. `page.rs` and
//! `retrieval.rs` supply that text via the SDK's `HttpsClient::get`.
//!
//! Dropped from the original: the `utils::storage_get`/`storage_set` caching
//! of extracted ops (this ABI exposes no persistent storage capability, so
//! every `resolve` call re-extracts ops from a freshly fetched player JS),
//! and `signatureTimestamp` extraction (only needed by the InnerTube
//! `playbackContext`, which this plugin never calls).
//!
//! Known limitation carried over unchanged from the source: YouTube's
//! separate "n" parameter throttling transform is not decoded. Streams
//! remain playable but may be rate-limited by the CDN; the upstream crate
//! has the same gap.

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CipherKind {
    Reverse,
    Splice,
    Swap,
}

#[derive(Debug, Clone)]
struct CipherOp {
    kind: CipherKind,
    index: usize,
}

/// An ordered sequence of cipher operations extracted from one player JS.
#[derive(Debug, Clone, Default)]
pub(crate) struct CipherOps(Vec<CipherOp>);

impl CipherOps {
    /// Apply all operations to decode a scrambled signature.
    fn decipher(&self, sig: &str) -> String {
        let mut chars: Vec<char> = sig.chars().collect();
        for op in &self.0 {
            match op.kind {
                CipherKind::Reverse => chars.reverse(),
                CipherKind::Splice => {
                    if op.index < chars.len() {
                        chars = chars[op.index..].to_vec();
                    }
                }
                CipherKind::Swap => {
                    if !chars.is_empty() {
                        let idx = op.index % chars.len();
                        chars.swap(0, idx);
                    }
                }
            }
        }
        chars.into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Cipher operation extraction (pure text parsing over player JS)
// ---------------------------------------------------------------------------

pub(crate) fn extract_cipher_ops(js: &str) -> Result<CipherOps, String> {
    if let Ok(ops) = extract_cipher_ops_packed_pool(js) {
        if !ops.is_empty() {
            return Ok(CipherOps(ops));
        }
    }
    if let Ok(ops) = extract_cipher_ops_youtubeexplode(js) {
        if !ops.is_empty() {
            return Ok(CipherOps(ops));
        }
    }
    if let Ok(ops) = extract_cipher_ops_fq(js) {
        if !ops.is_empty() {
            return Ok(CipherOps(ops));
        }
    }
    extract_cipher_ops_legacy(js).map(CipherOps)
}

// --- Packed constant-pool extraction (2026-08 player builds) ---------------
//
// Discovered live against `youtube.player.web_20260729_10_RC00`. The player
// no longer keeps its cipher-helper string constants in a literal quoted
// array (`extract_cipher_ops_fq`'s `h[]`); they are packed into ONE
// `{`-delimited string (e.g. `var B='replace{fromCharCode{...{splice{...
// {reverse{...'`) that gets indexed directly, and the 3-method splice/swap/
// reverse helper is invoked from a dispatcher via `helper[pool[X^C]](args)`
// where `X` is a runtime value never visible as a literal in the source.
//
// Every identifier here (`B`, the helper object, `X`) is minifier output and
// will differ per build, so nothing is name-matched — only shape-matched:
//   1. Any `var NAME='...'` whose pool tokens include exactly "splice" and
//      "reverse" is the constant pool.
//   2. Any `var NAME2={...}` object whose method bodies reference that pool,
//      classified by parameter count/body shape (not by name or by literal
//      "splice"/"reverse" text — the pool indirection hides that text).
//   3. The ordered `helper[pool[X^C]](args)` dispatch calls sharing one `X`.
//   4. `X` is unknowable statically, but the pool is small (tens of entries):
//      brute-force the one value of `X` that makes every dispatch call
//      resolve to a known helper key. This isn't defeating a security
//      boundary, only disambiguating an obfuscation — a bounded search over
//      a public, unencrypted string table.

struct DispatchCall {
    key_const: usize,
    second_arg: String,
}

fn extract_cipher_ops_packed_pool(js: &str) -> Result<Vec<CipherOp>, String> {
    let (pool_name, pool) = find_packed_pool(js)
        .ok_or_else(|| "Failed to locate packed constant-pool string".to_string())?;
    let (helper_name, methods) = find_pool_indexed_helper(js, &pool_name)
        .ok_or_else(|| "Failed to locate pool-indexed helper object".to_string())?;
    let calls = find_dispatch_calls(js, &helper_name, &pool_name)
        .ok_or_else(|| "Failed to locate dispatch call sequence".to_string())?;
    let base = solve_shared_base(&calls, &pool, &methods)
        .ok_or_else(|| "Could not solve the dispatcher's shared XOR base".to_string())?;

    let mut ops = Vec::with_capacity(calls.len());
    for call in &calls {
        let key_index = base ^ call.key_const;
        let key = pool
            .get(key_index)
            .ok_or("Resolved key index out of pool bounds")?;
        let kind = *methods
            .get(key.as_str())
            .ok_or("Dispatch call resolved to an unrecognized helper key")?;
        let index = match kind {
            CipherKind::Reverse => 0,
            _ => eval_arg(&call.second_arg, base)
                .ok_or("Could not evaluate dispatch call argument")?,
        };
        ops.push(CipherOp { kind, index });
    }
    if ops.is_empty() {
        return Err("No cipher operations resolved from dispatch calls".to_string());
    }
    Ok(ops)
}

/// Any `var NAME='...'` whose `{`-split value contains both "splice" and
/// "reverse" as exact tokens. Returns the pool's tokens in order.
fn find_packed_pool(js: &str) -> Option<(String, Vec<String>)> {
    let mut offset = 0usize;
    while let Some(local) = js[offset..].find("var ") {
        let var_pos = offset + local;
        let name_start = var_pos + 4;
        let Some(eq_rel) = js[name_start..].find('=') else {
            offset = name_start;
            continue;
        };
        let name = js[name_start..name_start + eq_rel].trim();
        let value_start = name_start + eq_rel + 1;
        if !name.is_empty() && name.chars().all(is_ident_char) {
            if let Some(literal) = js[value_start..].trim_start().strip_prefix('\'') {
                if let Some(end) = find_unescaped_quote(literal, '\'') {
                    let value = &literal[..end];
                    let tokens: Vec<String> = value.split('{').map(str::to_string).collect();
                    if tokens.iter().any(|t| t == "splice") && tokens.iter().any(|t| t == "reverse")
                    {
                        return Some((name.to_string(), tokens));
                    }
                }
            }
        }
        offset = name_start;
    }
    None
}

fn find_unescaped_quote(s: &str, quote: char) -> Option<usize> {
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' {
            escape = true;
            continue;
        }
        if c == quote {
            return Some(i);
        }
    }
    None
}

/// Any `var NAME2={...}` object literal whose body references `pool_name[`,
/// with `key:function(params){body}` properties classified by parameter
/// count and body shape: one param -> Reverse; two params with a `%`-modulo
/// self-swap shape (`X[0]=`) -> Swap; two params otherwise -> Splice.
fn find_pool_indexed_helper(
    js: &str,
    pool_name: &str,
) -> Option<(String, HashMap<String, CipherKind>)> {
    let marker = format!("{pool_name}[");
    let mut offset = 0usize;
    while let Some(local) = js[offset..].find("var ") {
        let var_pos = offset + local;
        let name_start = var_pos + 4;
        let Some(eq_rel) = js[name_start..].find('=') else {
            offset = name_start;
            continue;
        };
        let name = js[name_start..name_start + eq_rel].trim();
        let value_start = name_start + eq_rel + 1;
        if !name.is_empty()
            && name.chars().all(is_ident_char)
            && js[value_start..].trim_start().starts_with('{')
        {
            let brace_start = value_start + js[value_start..].find('{').unwrap();
            if let Some(body) = find_matching_brace(js, brace_start) {
                if body.contains(&marker) {
                    let methods = classify_pool_helper_methods(&body, pool_name);
                    if methods.len() == 3 {
                        return Some((name.to_string(), methods));
                    }
                }
            }
        }
        offset = name_start;
    }
    None
}

fn classify_pool_helper_methods(body: &str, pool_name: &str) -> HashMap<String, CipherKind> {
    let mut methods = HashMap::new();
    let mut pos = 0usize;
    while let Some(local) = body[pos..].find(":function(") {
        let colon_pos = pos + local;
        let key = body[..colon_pos]
            .rsplit(|c: char| c == ',' || c == '{' || c == '\n')
            .next()
            .unwrap_or("")
            .trim();
        let params_start = colon_pos + ":function(".len();
        let Some(params_end_rel) = body[params_start..].find(')') else {
            pos = colon_pos + 1;
            continue;
        };
        let params = &body[params_start..params_start + params_end_rel];
        let param_count = if params.trim().is_empty() {
            0
        } else {
            params.split(',').count()
        };
        let Some(body_open_rel) = body[params_start + params_end_rel..].find('{') else {
            pos = colon_pos + 1;
            continue;
        };
        let body_open = params_start + params_end_rel + body_open_rel;
        let Some(fn_body) = find_matching_brace(body, body_open) else {
            pos = colon_pos + 1;
            continue;
        };
        if fn_body.contains(pool_name) {
            let kind = match param_count {
                1 => Some(CipherKind::Reverse),
                2 if fn_body.contains('%') && fn_body.contains("[0]=") => Some(CipherKind::Swap),
                2 => Some(CipherKind::Splice),
                _ => None,
            };
            if let (Some(kind), false) = (kind, key.is_empty()) {
                methods.insert(key.to_string(), kind);
            }
        }
        pos = body_open + fn_body.len() + 2;
    }
    methods
}

/// Ordered `helper[pool[IDENT^CONST]](args)` calls sharing one symbolic
/// `IDENT` — the dispatcher's runtime XOR base, unknowable statically (see
/// `solve_shared_base`). This exact double-bracket shape combining the two
/// already-identified names is distinctive enough that a majority vote over
/// every match's `IDENT` is sufficient to isolate the real dispatch site.
fn find_dispatch_calls(js: &str, helper_name: &str, pool_name: &str) -> Option<Vec<DispatchCall>> {
    let prefix = format!("{helper_name}[{pool_name}[");
    let mut hits: Vec<(String, usize, String)> = Vec::new();
    let mut offset = 0usize;
    while let Some(local) = js[offset..].find(&prefix) {
        let pos = offset + local;
        let expr_start = pos + prefix.len();
        offset = expr_start;
        let Some(close_bracket_rel) = js[expr_start..].find(']') else {
            continue;
        };
        let expr = &js[expr_start..expr_start + close_bracket_rel];
        // Two closing brackets precede the call parens: one for `pool[...]`,
        // one for the outer `helper[...]` that wraps it.
        let after = &js[expr_start + close_bracket_rel + 1..];
        let (Some((ident, konst)), Some(rest)) =
            (parse_xor_expr(expr), after.strip_prefix("]("))
        else {
            continue;
        };
        let Some(close_paren_rel) = rest.find(')') else {
            continue;
        };
        let args = &rest[..close_paren_rel];
        let second_arg = args.splitn(2, ',').nth(1).unwrap_or("").trim().to_string();
        hits.push((ident, konst, second_arg));
    }
    if hits.len() < 2 {
        return None;
    }
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for (ident, _, _) in &hits {
        *counts.entry(ident.as_str()).or_insert(0) += 1;
    }
    let majority_ident = counts.into_iter().max_by_key(|(_, count)| *count)?.0.to_string();
    let calls: Vec<DispatchCall> = hits
        .into_iter()
        .filter(|(ident, _, _)| *ident == majority_ident)
        .map(|(_, konst, arg)| DispatchCall {
            key_const: konst,
            second_arg: arg,
        })
        .collect();
    if calls.len() < 2 { None } else { Some(calls) }
}

fn parse_xor_expr(expr: &str) -> Option<(String, usize)> {
    let (ident, konst) = expr.split_once('^')?;
    let ident = ident.trim();
    if ident.is_empty() || !ident.chars().all(is_ident_char) {
        return None;
    }
    let konst: usize = konst.trim().parse().ok()?;
    Some((ident.to_string(), konst))
}

/// Brute-forces the dispatcher's runtime XOR base: the value making every
/// dispatch call resolve, through the pool, to one of the helper's known
/// property keys. The pool holds tens of entries, so this is a cheap bounded
/// search — it disambiguates an obfuscation, not a real security boundary.
/// Fails closed (returns `None`) rather than guess if more than one value
/// satisfies every call, since that means the calls weren't specific enough
/// to pin down a unique answer.
fn solve_shared_base(
    calls: &[DispatchCall],
    pool: &[String],
    methods: &HashMap<String, CipherKind>,
) -> Option<usize> {
    let mut solution: Option<usize> = None;
    for base in 0..(1usize << 17) {
        let all_match = calls.iter().all(|call| {
            pool.get(base ^ call.key_const)
                .is_some_and(|key| methods.contains_key(key.as_str()))
        });
        if all_match {
            if solution.is_some() {
                return None;
            }
            solution = Some(base);
        }
    }
    solution
}

/// Evaluate a dispatch call's second argument: a bare integer literal (e.g.
/// a splice count), or an `IDENT^CONST` expression using the same symbolic
/// base `solve_shared_base` already resolved (e.g. a swap index).
fn eval_arg(arg: &str, base: usize) -> Option<usize> {
    let arg = arg.trim();
    if let Ok(n) = arg.parse::<usize>() {
        return Some(n);
    }
    let (_, konst) = parse_xor_expr(arg)?;
    Some(base ^ konst)
}

// --- YoutubeExplode-equivalent extraction (split/join + helper container) ---

fn extract_cipher_ops_youtubeexplode(js: &str) -> Result<Vec<CipherOp>, String> {
    let cipher_callsite =
        find_cipher_callsite(js).ok_or_else(|| "Failed to locate cipher callsite".to_string())?;
    let split_var = extract_split_var_name(&cipher_callsite)
        .ok_or_else(|| "Failed to identify split variable in callsite".to_string())?;
    let (container_name, _) = parse_first_container_call(&cipher_callsite, &split_var)
        .ok_or_else(|| "Failed to identify cipher helper container".to_string())?;
    let cipher_definition = find_cipher_container_definition(js, &container_name)
        .ok_or_else(|| "Failed to locate cipher helper definition".to_string())?;
    let method_types = classify_cipher_methods_youtubeexplode(&cipher_definition);
    if method_types.is_empty() {
        return Err("Failed to classify cipher helper methods".to_string());
    }
    let mut ops = Vec::new();
    for statement in cipher_callsite.split(';') {
        if let Some((_, method_name, index)) = parse_container_call(statement, &split_var) {
            if let Some(kind) = method_types.get(method_name) {
                ops.push(CipherOp {
                    kind: *kind,
                    index,
                });
            }
        }
    }
    if ops.is_empty() {
        return Err("No cipher operations were parsed from callsite".to_string());
    }
    Ok(ops)
}

fn find_cipher_callsite(js: &str) -> Option<String> {
    let mut offset = 0usize;
    while let Some(local) = js[offset..].find("=function(") {
        let func_pos = offset + local;
        let body_open = js[func_pos..].find('{').map(|p| func_pos + p)?;
        let body = find_matching_brace(js, body_open)?;
        if let Some(var_name) = extract_split_var_name(&body) {
            if body.contains(".split(\"\")") || body.contains(".split('')") {
                let return_join1 = format!("return {var_name}.join(\"\")");
                let return_join2 = format!("return {var_name}.join('')");
                if body.contains(&return_join1) || body.contains(&return_join2) {
                    return Some(body);
                }
            }
        }
        offset = body_open + 1;
    }
    None
}

fn extract_split_var_name(callsite_or_body: &str) -> Option<String> {
    let split_pos = callsite_or_body
        .find(".split(\"\")")
        .or_else(|| callsite_or_body.find(".split('')"))?;
    let lhs = &callsite_or_body[..split_pos];
    let eq_pos = lhs.rfind('=')?;
    let candidate = lhs[..eq_pos]
        .trim()
        .rsplit(|c: char| !is_ident_char(c))
        .next()
        .unwrap_or("")
        .trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

fn parse_first_container_call(callsite: &str, split_var: &str) -> Option<(String, String)> {
    for statement in callsite.split(';') {
        if let Some((container, method, _)) = parse_container_call(statement, split_var) {
            return Some((container.to_string(), method.to_string()));
        }
    }
    None
}

fn parse_container_call<'a>(
    statement: &'a str,
    split_var: &str,
) -> Option<(&'a str, &'a str, usize)> {
    let statement = statement.trim();
    let dot_pos = statement.find('.')?;
    let container = statement[..dot_pos].trim();
    if container.is_empty() {
        return None;
    }
    let after_dot = &statement[dot_pos + 1..];
    let open_paren = after_dot.find('(')?;
    let method = after_dot[..open_paren].trim();
    let args = after_dot[open_paren + 1..].trim_end_matches(')').trim();
    let mut arg_parts = args.split(',').map(|s| s.trim());
    let first = arg_parts.next()?;
    if first != split_var {
        return None;
    }
    let index = arg_parts
        .next()
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(0);
    Some((container, method, index))
}

fn find_cipher_container_definition(js: &str, container_name: &str) -> Option<String> {
    for token in [
        format!("var {container_name}={{"),
        format!("var {container_name} = {{"),
        format!("{container_name}={{"),
        format!("{container_name} = {{"),
    ] {
        if let Some(pos) = js.find(&token) {
            let brace_start = pos + token.len() - 1;
            return find_matching_brace(js, brace_start);
        }
    }
    None
}

fn classify_cipher_methods_youtubeexplode(definition: &str) -> HashMap<String, CipherKind> {
    let mut methods: HashMap<String, CipherKind> = HashMap::new();
    let mut pos = 0usize;
    while let Some(local) = definition[pos..].find(":function(") {
        let fn_pos = pos + local;
        let before = &definition[..fn_pos];
        let method_name = before
            .rsplit(|c: char| c == ',' || c == '{' || c == '\n' || c == ';')
            .next()
            .unwrap_or("")
            .trim();
        let body_open = definition[fn_pos..].find('{').map(|i| fn_pos + i);
        if let Some(body_open) = body_open {
            if let Some(body) = find_matching_brace(definition, body_open) {
                let kind = if body.contains('%') {
                    Some(CipherKind::Swap)
                } else if body.contains("splice") {
                    Some(CipherKind::Splice)
                } else if body.contains("reverse") {
                    Some(CipherKind::Reverse)
                } else {
                    None
                };
                if let (Some(kind), false) = (kind, method_name.is_empty()) {
                    methods.insert(method_name.to_string(), kind);
                }
                pos = body_open + body.len() + 2;
                continue;
            }
        }
        pos = fn_pos + 1;
    }
    methods
}

fn is_ident_char(c: char) -> bool {
    c == '_' || c == '$' || c.is_ascii_alphanumeric()
}

// --- Modern extraction: fQ dispatch + uC helper object + h[] string array ---

fn extract_cipher_ops_fq(js: &str) -> Result<Vec<CipherOp>, String> {
    let h_arr =
        extract_h_array(js).ok_or_else(|| "Failed to locate h[] array in player JS".to_string())?;
    if h_arr.is_empty() {
        return Err("h[] array is empty".to_string());
    }
    let splice_idx = h_arr
        .iter()
        .position(|s| s == "splice")
        .ok_or_else(|| "Could not find 'splice' in h[] array".to_string())?;
    let reverse_idx = h_arr
        .iter()
        .position(|s| s == "reverse")
        .ok_or_else(|| "Could not find 'reverse' in h[] array".to_string())?;
    let uc_methods = extract_uc_methods(js, splice_idx, reverse_idx)
        .map_err(|e| format!("uC method extraction failed: {e}"))?;
    if uc_methods.is_empty() {
        return Err("No uC cipher methods found".to_string());
    }
    let (v_outer, cipher_block) = extract_fq_cipher_block(js)
        .map_err(|e| format!("fQ cipher block extraction failed: {e}"))?;
    extract_ops_from_block(&cipher_block, v_outer, &h_arr, &uc_methods)
        .map_err(|e| format!("Cipher op parsing failed: {e}"))
}

fn extract_h_array(js: &str) -> Option<Vec<String>> {
    let bracket_pos = ["var h=[", "let h=[", "const h=[", "var h = ["]
        .iter()
        .find_map(|t| js.find(t).map(|p| p + t.len() - 1))?;
    let array_text = extract_bracketed_text(&js[bracket_pos..], '[', ']')?;
    Some(extract_quoted_strings(&array_text))
}

fn extract_bracketed_text(s: &str, open: char, close: char) -> Option<String> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = s.chars().collect();
    let mut start = None;
    for (i, &c) in chars.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if c == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if c == open {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                if let Some(s_idx) = start {
                    return Some(chars[s_idx + 1..i].iter().collect());
                }
            }
        }
    }
    None
}

fn extract_quoted_strings(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            let mut j = i + 1;
            let mut value = String::new();
            let mut escape = false;
            while j < chars.len() {
                let c = chars[j];
                if escape {
                    match c {
                        'n' => value.push('\n'),
                        't' => value.push('\t'),
                        'r' => value.push('\r'),
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        _ => {
                            value.push('\\');
                            value.push(c);
                        }
                    }
                    escape = false;
                } else if c == '\\' {
                    escape = true;
                } else if c == '"' {
                    break;
                } else {
                    value.push(c);
                }
                j += 1;
            }
            result.push(value);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    result
}

fn extract_uc_methods(
    js: &str,
    splice_idx: usize,
    reverse_idx: usize,
) -> Result<HashMap<String, CipherKind>, String> {
    let token = "var uC={";
    let pos = js
        .find(token)
        .ok_or_else(|| "Could not find 'var uC={' in player JS".to_string())?;
    let brace_start = pos + token.len() - 1;
    let inner = extract_bracketed_text(&js[brace_start..], '{', '}')
        .ok_or_else(|| "Could not find matching '}' for uC object".to_string())?;
    let mut methods: HashMap<String, CipherKind> = HashMap::new();
    let mut search = inner.as_str();
    while let Some(fn_pos) = search.find(":function(") {
        let before = &search[..fn_pos];
        let name_start = before
            .rfind(|c: char| c == ',' || c == '{' || c == '\n' || c == ';')
            .map(|p| p + 1)
            .unwrap_or(0);
        let name = before[name_start..].trim();
        if !name.is_empty() {
            let body_search_start = fn_pos + ":function(".len();
            if let Some(pc) = search[body_search_start..]
                .find("){")
                .map(|p| body_search_start + p + 1)
            {
                let body = extract_bracketed_text(&search[pc..], '{', '}').unwrap_or_default();
                if let Some(k) = classify_uc_body(&body, splice_idx, reverse_idx) {
                    methods.insert(name.to_string(), k);
                }
            }
        }
        search = &search[fn_pos + 1..];
    }
    Ok(methods)
}

fn classify_uc_body(body: &str, splice_idx: usize, reverse_idx: usize) -> Option<CipherKind> {
    if body.contains(&format!("h[{splice_idx}]")) {
        Some(CipherKind::Splice)
    } else if body.contains(&format!("h[{reverse_idx}]")) {
        Some(CipherKind::Reverse)
    } else if body.contains("D[0]") {
        Some(CipherKind::Swap)
    } else {
        None
    }
}

fn extract_fq_cipher_block(js: &str) -> Result<(usize, String), String> {
    let cipher_call_start = find_fq_cipher_call(js)
        .ok_or_else(|| "Could not find fQ cipher call pattern in player JS".to_string())?;
    let call_slice = &js[cipher_call_start..];
    let (d_outer, x_outer) = parse_fq_outer_args(call_slice)
        .ok_or_else(|| "Failed to parse fQ outer call arguments".to_string())?;
    let v_outer = x_outer ^ d_outer;
    let fq_fn_pos = js
        .find("fQ=function(D,X,B,C){")
        .or_else(|| js.find("fQ =function(D,X,B,C){"))
        .ok_or_else(|| "Could not find fQ function definition".to_string())?;
    let fq_brace = js[fq_fn_pos..]
        .find('{')
        .map(|p| fq_fn_pos + p)
        .ok_or_else(|| "Could not find opening brace of fQ function".to_string())?;
    let fq_body = extract_bracketed_text(&js[fq_brace..], '{', '}')
        .ok_or_else(|| "Could not extract fQ function body".to_string())?;
    let d80_marker = "if((D|80)==D)";
    let d80_pos = fq_body
        .find(d80_marker)
        .ok_or_else(|| "Could not find if((D|80)==D) block in fQ body".to_string())?;
    let d80_brace_start = fq_body[d80_pos..]
        .find('{')
        .map(|p| d80_pos + p)
        .ok_or_else(|| "No '{' after if((D|80)==D)".to_string())?;
    let cipher_block = extract_bracketed_text(&fq_body[d80_brace_start..], '{', '}')
        .ok_or_else(|| "Could not extract if((D|80)==D) block content".to_string())?;
    Ok((v_outer, cipher_block))
}

fn find_fq_cipher_call(js: &str) -> Option<usize> {
    let mut search = js;
    let mut base_offset = 0usize;
    while let Some(pos) = search.find("fQ(") {
        if is_fq_cipher_call(&search[pos..]) {
            return Some(base_offset + pos);
        }
        base_offset += pos + 3;
        search = &search[pos + 3..];
    }
    None
}

fn is_fq_cipher_call(slice: &str) -> bool {
    let after_fq = &slice[3..];
    if let Some((_, rest1)) = parse_number(after_fq) {
        if rest1.starts_with(',') {
            if let Some((_, rest3)) = parse_number(&rest1[1..]) {
                if rest3.starts_with(",fQ(") {
                    if let Some((_, rest5)) = parse_number(&rest3[4..]) {
                        if rest5.starts_with(',') {
                            if let Some((_, rest7)) = parse_number(&rest5[1..]) {
                                return rest7.starts_with(",m.s)");
                            }
                        }
                    }
                }
            }
        }
    }
    false
}

fn parse_fq_outer_args(slice: &str) -> Option<(usize, usize)> {
    let after_fq = slice.strip_prefix("fQ(")?;
    let (d, rest1) = parse_number(after_fq)?;
    let (x, _) = parse_number(rest1.strip_prefix(',')?)?;
    Some((d, x))
}

fn extract_ops_from_block(
    block: &str,
    v: usize,
    h_arr: &[String],
    uc_methods: &HashMap<String, CipherKind>,
) -> Result<Vec<CipherOp>, String> {
    let mut ops = Vec::new();
    let mut search = block;
    let marker = "uC[h[V^";
    while let Some(pos) = search.find(marker) {
        let after = &search[pos + marker.len()..];
        let (method_xor_const, rest1) = match parse_number(after) {
            Some(x) => x,
            None => {
                search = &search[pos + marker.len()..];
                continue;
            }
        };
        let rest2 = match rest1.strip_prefix("]](x") {
            Some(r) => r,
            None => {
                search = &search[pos + marker.len()..];
                continue;
            }
        };
        let actual_arg = if let Some(vx_rest) = rest2.strip_prefix(",V^") {
            parse_number(vx_rest).map(|(xc, _)| v ^ xc).unwrap_or(0)
        } else if let Some(lit_rest) = rest2.strip_prefix(',') {
            parse_number(lit_rest).map(|(n, _)| n).unwrap_or(0)
        } else {
            0
        };
        let method_h_idx = v ^ method_xor_const;
        let method_name = h_arr.get(method_h_idx).map(|s| s.as_str()).unwrap_or("");
        if let Some(kind) = uc_methods.get(method_name) {
            ops.push(CipherOp {
                kind: *kind,
                index: actual_arg,
            });
        }
        search = &search[pos + marker.len()..];
    }
    if ops.is_empty() {
        Err("No cipher operations found in fQ dispatch block".to_string())
    } else {
        Ok(ops)
    }
}

fn parse_number(s: &str) -> Option<(usize, &str)> {
    let end = s
        .char_indices()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    if end == 0 {
        return None;
    }
    let n: usize = s[..end].parse().ok()?;
    Some((n, &s[end..]))
}

// --- Legacy extraction (pre-2024 classic split/join helper) ---

fn extract_cipher_ops_legacy(js: &str) -> Result<Vec<CipherOp>, String> {
    let fn_body = find_decipher_function_body_legacy(js)
        .ok_or("Could not find decipher function in player JS")?;
    let helper_name = find_helper_object_name_legacy(&fn_body)
        .ok_or("Could not find helper object name in decipher function")?;
    let method_types = classify_helper_methods_legacy(js, &helper_name)?;
    parse_cipher_calls_legacy(&fn_body, &method_types)
}

fn find_decipher_function_body_legacy(js: &str) -> Option<String> {
    let split_marker = "a=a.split(\"\");";
    let join_marker = "return a.join(\"\")";
    let split_pos = js.find(split_marker)?;
    let after_split = &js[split_pos..];
    let join_offset = after_split.find(join_marker)?;
    let body_start = split_pos + split_marker.len();
    let body_end = split_pos + join_offset;
    let body = js[body_start..body_end].trim().trim_end_matches(';');
    Some(body.to_string())
}

fn find_helper_object_name_legacy(fn_body: &str) -> Option<String> {
    let dot_pos = fn_body.find('.')?;
    let name = fn_body[..dot_pos].trim().trim_start_matches(';');
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn classify_helper_methods_legacy(
    js: &str,
    helper_name: &str,
) -> Result<Vec<(String, CipherKind)>, String> {
    let mut obj_start = None;
    for pattern in &[
        format!("var {helper_name}={{"),
        format!("{helper_name} = {{"),
        format!("{helper_name}={{"),
    ] {
        if let Some(pos) = js.find(pattern.as_str()) {
            obj_start = Some(pos + pattern.len() - 1);
            break;
        }
    }
    let obj_start =
        obj_start.ok_or_else(|| format!("Could not find helper object '{helper_name}'"))?;
    let obj_body =
        find_matching_brace(js, obj_start).ok_or("Could not find end of helper object")?;
    let mut methods = Vec::new();
    let mut pos = 0;
    while pos < obj_body.len() {
        if let Some(colon_pos) = obj_body[pos..].find(":function(") {
            let name_start = obj_body[..pos + colon_pos]
                .rfind(|c: char| c == ',' || c == '{' || c == '\n')
                .map(|p| p + 1)
                .unwrap_or(pos);
            let method_name = obj_body[name_start..pos + colon_pos].trim().to_string();
            let fn_start_search = pos + colon_pos + ":function(".len();
            if let Some(paren_close) = obj_body[fn_start_search..].find("){") {
                let body_start = fn_start_search + paren_close + 1;
                if let Some(body) = find_matching_brace(&obj_body, body_start) {
                    if let Some(kind) = classify_method_body_legacy(&body) {
                        methods.push((method_name, kind));
                    }
                    pos = body_start + body.len() + 2;
                    continue;
                }
            }
        }
        pos += 1;
    }
    if methods.is_empty() {
        return Err("No cipher methods found in helper object".to_string());
    }
    Ok(methods)
}

fn classify_method_body_legacy(body: &str) -> Option<CipherKind> {
    if body.contains("reverse") {
        Some(CipherKind::Reverse)
    } else if body.contains("splice") {
        Some(CipherKind::Splice)
    } else if body.contains("var c=") || (body.contains("a[0]") && body.contains("a[b")) {
        Some(CipherKind::Swap)
    } else {
        None
    }
}

fn parse_cipher_calls_legacy(
    fn_body: &str,
    method_types: &[(String, CipherKind)],
) -> Result<Vec<CipherOp>, String> {
    let mut ops = Vec::new();
    for call in fn_body.split(';') {
        let call = call.trim();
        if call.is_empty() {
            continue;
        }
        if let Some(dot_pos) = call.find('.') {
            let after_dot = &call[dot_pos + 1..];
            if let Some(paren_pos) = after_dot.find('(') {
                let method_name = &after_dot[..paren_pos];
                let args_str = &after_dot[paren_pos + 1..].trim_end_matches(')');
                if let Some((_, kind)) = method_types.iter().find(|(n, _)| n == method_name) {
                    let index = args_str
                        .split(',')
                        .nth(1)
                        .and_then(|s| s.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    ops.push(CipherOp {
                        kind: *kind,
                        index,
                    });
                }
            }
        }
    }
    if ops.is_empty() {
        return Err("No cipher operations parsed from function body".to_string());
    }
    Ok(ops)
}

fn find_matching_brace(s: &str, start: usize) -> Option<String> {
    if s.as_bytes().get(start) != Some(&b'{') {
        return None;
    }
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(s[start + 1..start + i].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Signature-cipher string decoding
// ---------------------------------------------------------------------------

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(value) = u8::from_str_radix(&input[i + 1..i + 3], 16) {
                out.push(value);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn parse_query_string(qs: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for pair in qs.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(percent_decode(key), percent_decode(value));
        }
    }
    map
}

fn append_or_replace_query_param(url: &str, key: &str, value: &str) -> String {
    let mut parts = url.splitn(2, '?');
    let base = parts.next().unwrap_or(url);
    let query = parts.next().unwrap_or("");
    let mut kv_pairs = Vec::<(String, String)>::new();
    let mut replaced = false;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                kv_pairs.push((k.to_string(), value.to_string()));
                replaced = true;
            } else {
                kv_pairs.push((k.to_string(), v.to_string()));
            }
        } else {
            kv_pairs.push((pair.to_string(), String::new()));
        }
    }
    if !replaced {
        kv_pairs.push((key.to_string(), value.to_string()));
    }
    if kv_pairs.is_empty() {
        return base.to_string();
    }
    let query_string = kv_pairs
        .into_iter()
        .map(|(k, v)| if v.is_empty() { k } else { format!("{k}={v}") })
        .collect::<Vec<_>>()
        .join("&");
    format!("{base}?{query_string}")
}

/// Decode a `signatureCipher`/`cipher` query-string blob (`s=...&sp=...&url=...`)
/// into a playable URL, using the ops extracted from the matching player JS.
pub(crate) fn decode_signature_url(cipher_str: &str, ops: &CipherOps) -> Result<String, String> {
    let parts = parse_query_string(cipher_str);
    let enc_sig = parts
        .get("s")
        .or_else(|| parts.get("sig"))
        .ok_or("Missing 's' (signature) in signatureCipher")?;
    let base_url = parts.get("url").ok_or("Missing 'url' in signatureCipher")?;
    let sp = parts.get("sp").map(|s| s.as_str()).unwrap_or("signature");
    let decoded_sig = ops.decipher(enc_sig);
    let encoded_sig = percent_encode(&decoded_sig);
    let mut url = append_or_replace_query_param(base_url, sp, &encoded_sig);
    if !url.contains("ratebypass=") {
        url = append_or_replace_query_param(&url, "ratebypass", "yes");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../fixtures/player-legacy-cipher.js");
    const PACKED_POOL_FIXTURE: &str = include_str!("../fixtures/player-packed-pool-cipher.js");

    #[test]
    fn extracts_and_applies_synthetic_cipher_ops() {
        let ops = extract_cipher_ops(FIXTURE).expect("ops must extract");
        // Reverse, then splice(2), then swap(61) — see fixtures/README below.
        assert_eq!(ops.decipher("abcdefghij"), "cgfedhba");
    }

    #[test]
    fn extracts_and_applies_packed_pool_cipher_ops() {
        // Verified live against a real 2026-08 YouTube player build before
        // this fixture was written — see the module doc comment above
        // `extract_cipher_ops_packed_pool`. Splice(2), then reverse, then
        // swap(5) — see the fixture file's own header for the layout.
        let ops = extract_cipher_ops(PACKED_POOL_FIXTURE).expect("ops must extract");
        assert_eq!(ops.decipher("abcdefghij"), "eihgfjdc");
    }

    #[test]
    fn packed_pool_strategy_runs_before_the_older_strategies() {
        // The packed-pool fixture also happens to contain no `.split("")`/
        // `.join("")` pair, so the older strategies could never match it
        // anyway — this asserts the dispatcher's ordering intent directly,
        // independent of that fixture-specific coincidence.
        let direct = extract_cipher_ops_packed_pool(PACKED_POOL_FIXTURE)
            .expect("packed-pool strategy must succeed on its own fixture");
        assert_eq!(direct.len(), 3);
    }

    #[test]
    fn rejects_js_with_no_recognizable_cipher_pattern() {
        assert!(extract_cipher_ops("var x = 1;").is_err());
    }

    #[test]
    fn decodes_signature_cipher_query_into_playable_url() {
        let ops = extract_cipher_ops(FIXTURE).expect("ops must extract");
        let cipher = "s=abcdefghij&sp=sig&url=https%3A%2F%2Fexample.com%2Fvideo";
        let url = decode_signature_url(cipher, &ops).unwrap();
        assert_eq!(url, "https://example.com/video?sig=cgfedhba&ratebypass=yes");
    }
}

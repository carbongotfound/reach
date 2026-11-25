//! Loopback HTTP server: GET /health, POST /run (/cli), POST /mcp (JSON-RPC,
//! protocol 2024-11-05). Same routes, auth and JSON shapes as the C# build.

use crate::{config, util::base64, ActResult, Engine};
use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Runs before every tool call (CLI and MCP). Return Some to answer instead of the engine.
/// The app uses it to refuse calls outside a run and to serve its own tools.
pub type Hook = Arc<dyn Fn(&str, &mut Map<String, Value>) -> Option<ActResult> + Send + Sync>;
/// Runs on every tool result before it goes back to the client (for example to add a note).
pub type After = Arc<dyn Fn(&str, &mut ActResult) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Opts {
    /// Bearer token; None = the token in config.
    pub token: Option<String>,
    pub hook: Option<Hook>,
    /// Extra MCP tool schemas appended to tools/list (served through `hook`).
    pub extra_tools: Vec<Value>,
    pub after: Option<After>,
}

#[derive(Clone)]
struct St {
    engine: Engine,
    opts: Opts,
    clients: Arc<Mutex<(HashMap<String, String>, Option<String>)>>, // session -> client name, last client
}

/// Serves on `l` from a background thread. Returns the port.
pub fn start(engine: Engine, l: std::net::TcpListener, opts: Opts) -> Result<u16, String> {
    l.set_nonblocking(true).map_err(|e| e.to_string())?;
    let port = l.local_addr().map(|a| a.port()).map_err(|e| e.to_string())?;
    let st = St { engine, opts, clients: Default::default() };
    std::thread::Builder::new().name("reach-http".into()).spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(4).enable_io().build().expect("tokio");
        rt.block_on(async move {
            let l = tokio::net::TcpListener::from_std(l).expect("listener");
            let app = axum::Router::new().fallback(move |m: Method, u: Uri, h: HeaderMap, b: Bytes| serve(st.clone(), m, u, h, b));
            let _ = axum::serve(l, app).await;
        });
    }).map_err(|e| e.to_string())?;
    config::log(&format!("listening on http://127.0.0.1:{port}/ (token required)"));
    Ok(port)
}

fn reply(code: u16, body: String) -> Response {
    let mut r = (StatusCode::from_u16(code).unwrap_or(StatusCode::OK), body).into_response();
    r.headers_mut().insert("content-type", HeaderValue::from_static("application/json; charset=utf-8"));
    r
}

fn authed(h: &HeaderMap, token: &Option<String>) -> bool {
    let Some(v) = h.get("authorization").and_then(|v| v.to_str().ok()) else { return false };
    let v = if v.len() > 7 && v[..7].eq_ignore_ascii_case("bearer ") { v[7..].trim() } else { v };
    let t = token.clone().unwrap_or_else(|| config::get().token);
    // constant time
    v.len() == t.len() && v.bytes().zip(t.bytes()).fold(0u8, |d, (a, b)| d | (a ^ b)) == 0
}

async fn serve(st: St, m: Method, u: Uri, h: HeaderMap, body: Bytes) -> Response {
    let mut r = route(st, m, u, h, body).await;
    r.headers_mut().insert("server", HeaderValue::from_str(&format!("Reach/{}", config::VERSION)).unwrap());
    r
}

async fn route(st: St, m: Method, u: Uri, h: HeaderMap, body: Bytes) -> Response {
    let path = u.path().trim_end_matches('/').to_lowercase();
    let path = if path.is_empty() { "/".to_string() } else { path };
    if m == Method::GET && (path == "/health" || path == "/status") {
        let c = config::get();
        let t = crate::status(false).text;
        return reply(200, json!({ "ok": true, "paused": c.paused, "port": c.port, "text": t }).to_string());
    }
    if !authed(&h, &st.opts.token) {
        let mut r = reply(401, r#"{"error":"unauthorized"}"#.into());
        r.headers_mut().insert("www-authenticate", HeaderValue::from_static("Bearer"));
        return r;
    }
    let header = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()).map(String::from);
    if m == Method::POST && (path == "/run" || path == "/cli") {
        let d: Map<String, Value> = serde_json::from_slice(&body).ok().and_then(|v: Value| v.as_object().cloned()).unwrap_or_default();
        let tool = d.get("tool").or(d.get("cmd")).map(as_str).unwrap_or_default();
        let mut args = d.get("args").or(d.get("arguments")).and_then(|v| v.as_object().cloned()).unwrap_or_default();
        for (k, v) in &d {
            if !matches!(k.as_str(), "tool" | "cmd" | "args" | "arguments") { args.insert(k.clone(), v.clone()); }
        }
        if !args.contains_key("agent") { if let Some(a) = header("x-reach-agent") { args.insert("agent".into(), a.into()); } }
        let r = run(&st, tool, args).await;
        let mut o = Map::new();
        o.insert("ok".into(), r.ok.into());
        o.insert("text".into(), r.text.clone().into());
        if let Some(p) = &r.image_path { o.insert("path".into(), p.clone().into()); }
        if let Some(i) = &r.image { o.insert("bytes".into(), i.len().into()); }
        return reply(if r.ok { 200 } else { 400 }, Value::Object(o).to_string());
    }
    if path == "/mcp" {
        if m != Method::POST { return StatusCode::METHOD_NOT_ALLOWED.into_response(); }
        return mcp(st, h, body).await;
    }
    reply(404, r#"{"error":"not found"}"#.into())
}

fn as_str(v: &Value) -> String { match v { Value::String(s) => s.clone(), Value::Null => String::new(), o => o.to_string() } }

async fn run(st: &St, tool: String, mut args: Map<String, Value>) -> ActResult {
    let e = st.engine.clone();
    let (hook, after) = (st.opts.hook.clone(), st.opts.after.clone());
    tokio::task::spawn_blocking(move || {
        let mut r = if let Some(r) = hook.as_ref().and_then(|h| h(&tool, &mut args)) { r }
            else if tool == "batch" { batch(&e, hook.as_ref(), &args) }
            else { e.run(&tool, args) };
        if let Some(a) = after.as_ref() { a(&tool, &mut r); }
        r
    }).await.unwrap_or_else(|_| ActResult::fail("internal error"))
}

/// Several actions in one call, run in order through the same gate as single calls.
/// Stops at the first failure so nothing runs on a screen the agent didn't expect.
fn batch(e: &crate::Engine, hook: Option<&Hook>, args: &Map<String, Value>) -> ActResult {
    let acts = args.get("actions").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if acts.is_empty() || acts.len() > 10 { return ActResult::fail("batch needs 1 to 10 actions."); }
    let mut lines = Vec::new();
    for (i, a) in acts.into_iter().enumerate() {
        let Some(mut m) = a.as_object().cloned() else { return ActResult::fail(format!("Action {} is not an object.", i + 1)) };
        let t = m.remove("tool").map(|v| as_str(&v)).unwrap_or_default();
        if !matches!(t.as_str(), "apps" | "mouse" | "keyboard" | "ui") {
            lines.push(format!("{}. {t}: not allowed in a batch (use apps, mouse, keyboard or ui; take screenshots on their own). Nothing after it was run.", i + 1));
            return ActResult::fail(lines.join("\n"));
        }
        if let Some(ag) = args.get("agent") { m.entry("agent").or_insert(ag.clone()); }
        let act = m.get("action").map(as_str).unwrap_or_default();
        let r = match hook.and_then(|h| h(&t, &mut m)) { Some(r) => r, None => e.run(&t, m) };
        lines.push(format!("{}. {t} {act}: {}", i + 1, r.text.lines().next().unwrap_or("")));
        if !r.ok {
            if i + 1 < args["actions"].as_array().map_or(0, |v| v.len()) { lines.push(format!("Stopped at action {}; the rest were not run.", i + 1)); }
            return ActResult::fail(lines.join("\n"));
        }
    }
    ActResult::say(lines.join("\n"))
}

fn rpc_ok(id: &Value, result: Value) -> String { json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string() }

async fn mcp(st: St, h: HeaderMap, body: Bytes) -> Response {
    let Ok(msg) = serde_json::from_slice::<Value>(&body) else {
        return reply(400, r#"{"jsonrpc":"2.0","error":{"code":-32700,"message":"parse error"},"id":null}"#.into());
    };
    let id = msg.get("id").cloned().unwrap_or(Value::Null);
    let method = msg.get("method").map(as_str).unwrap_or_default();
    let params = msg.get("params").and_then(|p| p.as_object().cloned()).unwrap_or_default();
    match method.as_str() {
        "initialize" => {
            let ci = params.get("clientInfo");
            let client = ci.and_then(|c| c.get("title").or(c.get("name"))).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()).map(String::from);
            let mut sid = None;
            if let Some(c) = client {
                let s = crate::util::random_hex(16);
                let mut g = st.clients.lock().unwrap();
                if g.0.len() >= 200 { g.0.clear(); }
                g.0.insert(s.clone(), c.clone());
                g.1 = Some(c);
                sid = Some(s);
            }
            let res = json!({ "protocolVersion": "2024-11-05", "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "reach", "version": config::VERSION }, "instructions": instructions() });
            let mut r = reply(200, rpc_ok(&id, res));
            if let Some(s) = sid { r.headers_mut().insert("mcp-session-id", HeaderValue::from_str(&s).unwrap()); }
            r
        }
        m if m.starts_with("notifications/") => StatusCode::ACCEPTED.into_response(),
        "ping" => reply(200, rpc_ok(&id, json!({}))),
        "tools/list" => {
            let mut tools = tool_schemas();
            tools.as_array_mut().unwrap().extend(st.opts.extra_tools.iter().cloned());
            reply(200, rpc_ok(&id, json!({ "tools": tools })))
        }
        "tools/call" => {
            let name = params.get("name").map(as_str).unwrap_or_default();
            let mut args = params.get("arguments").and_then(|a| a.as_object().cloned()).unwrap_or_default();
            if !args.contains_key("agent") {
                let header = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).filter(|s| !s.is_empty()).map(String::from);
                let g = st.clients.lock().unwrap();
                let who = header("x-reach-agent")
                    .or_else(|| header("mcp-session-id").and_then(|s| g.0.get(&s).cloned()))
                    .or_else(|| g.1.clone());
                drop(g);
                if let Some(w) = who { args.insert("agent".into(), w.into()); }
            }
            let r = run(&st, name, args).await;
            reply(200, rpc_ok(&id, mcp_content(&r)))
        }
        _ => reply(200, json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32601, "message": "unknown method" } }).to_string()),
    }
}

pub fn mcp_content(r: &ActResult) -> Value {
    let mut content = vec![json!({ "type": "text", "text": r.text })];
    if let Some(i) = r.image.as_ref().filter(|i| !i.is_empty()) {
        content.push(json!({ "type": "image", "data": base64(i), "mimeType": r.mime }));
    }
    let mut o = json!({ "content": content });
    if !r.ok { o["isError"] = true.into(); }
    o
}

pub fn instructions() -> &'static str {
    "Drives this Windows desktop. Coordinates are screenshot image pixels; never scale them. \
Prefer apps open <name> and ui find/invoke (refs eN stay valid) over pixel clicks. \
Screenshot only when you need to see a result; 'unchanged' means nothing changed. Use batch for several steps whose result you already know (type into a known field, Tab, a known shortcut)."
}

fn prop(name: &str, ty: &str, desc: &str) -> (String, Value) {
    let v = if ty == "array" { json!({ "type": "array", "items": { "type": "integer" }, "description": desc }) }
            else { json!({ "type": ty, "description": desc }) };
    (name.into(), v)
}
fn prop_enum(name: &str, values: &str) -> (String, Value) {
    let ev: Vec<&str> = values.split([',', ' ']).filter(|s| !s.is_empty()).collect();
    (name.into(), json!({ "type": "string", "enum": ev }))
}
fn tool(name: &str, desc: &str, props: Vec<(String, Value)>, required: Option<&str>) -> Value {
    let mut schema = json!({ "type": "object", "properties": Value::Object(props.into_iter().collect()) });
    if let Some(r) = required { schema["required"] = json!([r]); }
    json!({ "name": name, "description": desc, "inputSchema": schema })
}

pub fn tool_schemas() -> Value {
    json!([
        tool("screenshot", "JPEG of the desktop, a window, or a region, plus image size and cursor. Mouse coordinates are always in the full-desktop screenshot's space; a window or region shot says where it sits in that space (click box x,y wxh).", vec![
            prop("window", "string", "hwnd from apps"), prop("region", "array", "[x,y,w,h] zoom"),
            prop("max", "integer", "long edge px, default 1280"), prop("quality", "integer", "JPEG 20-95"),
            prop("force", "boolean", "image even if unchanged")], None),
        tool("apps", "List, open, focus, close, min/max/restore windows; list displays.", vec![
            prop_enum("action", "list, open, focus, close, minimize, maximize, restore, displays"),
            prop("name", "string", "app name, or title/process"), prop("window", "string", "hwnd"),
            prop("filter", "string", "list filter"), prop("new", "boolean", "open a new instance"),
            prop("json", "boolean", "full detail")], Some("action")),
        tool("mouse", "Click, move, drag, scroll at [x,y] image coords.", vec![
            prop_enum("action", "click, right_click, double_click, middle_click, move, drag, scroll, position, down, up, wait"),
            prop("coordinate", "array", "[x,y]"), prop("to", "array", "drag end [x,y]"),
            prop_enum("direction", "up, down, left, right"), prop("amount", "integer", "scroll notches, default 3"),
            prop("duration", "number", "wait seconds, max 10"), prop("fast", "boolean", "quick motion this call")], Some("action")),
        tool("keyboard", "Type text, or press chords like Return, ctrl+shift+t, win+r; sequences \"ctrl+c, ctrl+v\".", vec![
            prop_enum("action", "type, key, hold"), prop("text", "string", "text or chord"),
            prop("hold", "integer", "key: hold ms"), prop("duration", "number", "hold seconds, max 10"),
            prop("fast", "boolean", "quick typing this call")], Some("action")),
        tool("ui", "Accessibility tree. find/inspect return refs (eN) for invoke, set, read, toggle, focus. ref may also be a control name.", vec![
            prop_enum("action", "inspect, find, invoke, set, read, toggle, focus"), prop("query", "string", "find text"),
            prop("ref", "string", "eN or control name"), prop("value", "string", "text for set"),
            prop("window", "string", "hwnd, default focused window"), prop("limit", "integer", "default 60 inspect, 25 find"),
            prop("all", "boolean", "no filtering"), prop("json", "boolean", "full detail")], Some("action")),
        json!({ "name": "batch", "description": "Run up to 10 apps/mouse/keyboard/ui actions in order in one call, when each result is predictable. Stops at the first failure. No screenshots inside; take one after if needed.",
            "inputSchema": { "type": "object", "required": ["actions"], "properties": { "actions": { "type": "array", "maxItems": 10,
                "description": "e.g. [{\"tool\":\"ui\",\"action\":\"set\",\"ref\":\"e3\",\"value\":\"Acme\"},{\"tool\":\"keyboard\",\"action\":\"key\",\"text\":\"tab\"}]",
                "items": { "type": "object", "required": ["tool", "action"], "properties": { "tool": { "type": "string", "enum": ["apps", "mouse", "keyboard", "ui"] }, "action": { "type": "string" } } } } } } }),
    ])
}

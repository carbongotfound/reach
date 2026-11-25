//! `reach <command>`: builds the same JSON payloads as the C# CLI and POSTs them
//! to the local server, starting it if needed.

use reach_core::config;
use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

type R<T> = Result<T, String>;

pub fn run(args: Vec<String>) -> R<i32> {
    if args.is_empty() || is_help(&args[0]) { print_help(); return Ok(0); }
    let cmd = args[0].to_lowercase();
    if cmd == "stop" { println!("Use the tray icon > Exit to stop Reach."); return Ok(0); }
    let mut agent = std::env::var("REACH_AGENT").unwrap_or_default();
    let (mut no_cursor, mut no_yield, mut fast) = (false, false, false);
    let args = strip_global_flags(args, &mut agent, &mut no_cursor, &mut no_yield, &mut fast);
    if args.is_empty() { print_help(); return Ok(0); }
    let mut d = build_payload(&args)?;
    if fast { d.insert("fast".into(), true.into()); }
    if no_yield { d.insert("no_yield".into(), true.into()); }
    if !agent.trim().is_empty() { d.insert("agent".into(), agent.trim().into()); }
    if no_cursor { d.insert("no_cursor".into(), true.into()); }
    let payload = Value::Object(d).to_string();
    let body = match post("/run", &payload) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::ConnectionRefused || e.kind() == std::io::ErrorKind::TimedOut => {
            ensure_server()?;
            post("/run", &payload).map_err(|e| e.to_string())?
        }
        Err(e) => return Err(e.to_string()),
    };
    let Ok(Value::Object(d)) = serde_json::from_str::<Value>(&body) else { eprintln!("{body}"); return Ok(1) };
    let ok = d.get("ok").and_then(|v| v.as_bool()).unwrap_or(true);
    match d.get("text") { Some(Value::String(t)) => println!("{t}"), Some(t) => println!("{t}"), None => println!("{body}") }
    if let Some(p) = d.get("path") { println!("path {}", p.as_str().unwrap_or_default()); }
    Ok(if ok { 0 } else { 2 })
}

/// --agent, --no-cursor, --no-yield, --fast: anywhere, except after a free-text command.
fn strip_global_flags(args: Vec<String>, agent: &mut String, nc: &mut bool, ny: &mut bool, fast: &mut bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut cmd: Option<String> = None;
    let mut it = args.into_iter().peekable();
    while let Some(a) = it.next() {
        let free = matches!(cmd.as_deref(), Some("type" | "key" | "open" | "ui" | "apps" | "windows"));
        if !free {
            if (a == "--agent" || a == "-a") && it.peek().is_some() { *agent = it.next().unwrap(); continue; }
            if let Some(v) = a.strip_prefix("--agent=") { *agent = v.into(); continue; }
            match a.as_str() { "--no-cursor" => { *nc = true; continue } "--no-yield" => { *ny = true; continue } "--fast" => { *fast = true; continue } _ => {} }
        }
        if cmd.is_none() { cmd = Some(a.to_lowercase()); }
        out.push(a);
    }
    out
}

fn is_help(s: &str) -> bool { matches!(s.to_lowercase().as_str(), "-h" | "--help" | "help" | "/?") }

fn int(s: &str) -> R<i64> { s.trim().parse::<i64>().map_err(|_| "Input string was not in a correct format.".to_string()) }
fn join(a: &[String], start: usize) -> String { if a.len() <= start { String::new() } else { a[start..].join(" ") } }

fn build_payload(args: &[String]) -> R<Map<String, Value>> {
    let cmd = args[0].to_lowercase();
    let mut d = Map::new();
    let set = |d: &mut Map<String, Value>, k: &str, v: Value| { d.insert(k.into(), v); };
    let win = |a: &[String]| Value::from(a.get(1).cloned().unwrap_or_default());
    match cmd.as_str() {
        "screenshot" | "shot" => { set(&mut d, "tool", "screenshot".into()); parse_flags(args, 1, &mut d)?; }
        "zoom" => {
            set(&mut d, "tool", "screenshot".into());
            if args.len() < 5 { return Err("usage: reach zoom X Y W H".into()); }
            set(&mut d, "region", Value::from(vec![int(&args[1])?, int(&args[2])?, int(&args[3])?, int(&args[4])?]));
            parse_flags(args, 5, &mut d)?;
        }
        "apps" | "windows" => {
            set(&mut d, "tool", "apps".into()); set(&mut d, "action", "list".into());
            let rest = take_list_flags(args, &mut d)?;
            if rest.len() > 1 { set(&mut d, "filter", join(&rest, 1).into()); }
        }
        "open" => {
            set(&mut d, "tool", "apps".into()); set(&mut d, "action", "open".into());
            let n = join(args, 1);
            if n.is_empty() { return Err("usage: reach open <app>".into()); }
            set(&mut d, "name", n.into());
        }
        "focus" | "close" | "minimize" | "maximize" => { set(&mut d, "tool", "apps".into()); set(&mut d, "action", cmd.clone().into()); set(&mut d, "window", win(args)); }
        "displays" => { set(&mut d, "tool", "apps".into()); set(&mut d, "action", "displays".into()); }
        "click" | "right-click" | "rightclick" | "double-click" | "doubleclick" | "move" => {
            let action = match cmd.as_str() { "click" => "click", "move" => "move", "right-click" | "rightclick" => "right_click", _ => "double_click" };
            set(&mut d, "tool", "mouse".into()); set(&mut d, "action", action.into());
            if args.len() < 3 { return Err(format!("usage: reach {} X Y", args[0])); }
            set(&mut d, "coordinate", Value::from(vec![int(&args[1])?, int(&args[2])?]));
        }
        "drag" => {
            set(&mut d, "tool", "mouse".into()); set(&mut d, "action", "drag".into());
            if args.len() < 5 { return Err("usage: reach drag X1 Y1 X2 Y2".into()); }
            set(&mut d, "coordinate", Value::from(vec![int(&args[1])?, int(&args[2])?]));
            set(&mut d, "to", Value::from(vec![int(&args[3])?, int(&args[4])?]));
        }
        "scroll" => {
            set(&mut d, "tool", "mouse".into()); set(&mut d, "action", "scroll".into());
            set(&mut d, "direction", args.get(1).cloned().unwrap_or_else(|| "down".into()).into());
            if args.len() > 2 { set(&mut d, "amount", int(&args[2])?.into()); }
            if args.len() > 4 { set(&mut d, "coordinate", Value::from(vec![int(&args[3])?, int(&args[4])?])); }
        }
        "type" => { set(&mut d, "tool", "keyboard".into()); set(&mut d, "action", "type".into()); set(&mut d, "text", join(args, 1).into()); }
        "key" => {
            set(&mut d, "tool", "keyboard".into()); set(&mut d, "action", "key".into());
            let mut rest = Vec::new();
            let mut i = 1;
            while i < args.len() {
                if args[i] == "--hold" && i + 1 < args.len() { i += 1; set(&mut d, "hold", int(&args[i])?.into()); }
                else if args[i] == "--fast" { set(&mut d, "fast", true.into()); }
                else { rest.push(args[i].clone()); }
                i += 1;
            }
            set(&mut d, "text", rest.join(" ").into());
        }
        "motion" => { set(&mut d, "tool", "motion".into()); set(&mut d, "value", args.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "status".into()).into()); }
        "ui" => {
            set(&mut d, "tool", "ui".into());
            let a: Vec<String> = if args.len() > 1 && (args[1] == "find" || args[1] == "inspect") { take_list_flags(args, &mut d)? } else { args.to_vec() };
            let action = a.get(1).cloned().unwrap_or_else(|| "inspect".into());
            set(&mut d, "action", action.clone().into());
            if action == "find" && a.len() > 2 { set(&mut d, "query", join(&a, 2).into()); }
            else if action == "set" && a.len() > 3 { set(&mut d, "ref", a[2].clone().into()); set(&mut d, "value", join(&a, 3).into()); }
            else if a.len() > 2 { set(&mut d, "ref", a[2].clone().into()); }
        }
        "status" => { set(&mut d, "tool", "status".into()); take_list_flags(args, &mut d)?; }
        "pause-on-user" | "pauseonuser" => {
            set(&mut d, "tool", "pause_on_user".into());
            set(&mut d, "action", args.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "status".into()).into());
            if args.len() > 2 { set(&mut d, "value", int(&args[2])?.into()); }
        }
        "cursor" => {
            set(&mut d, "tool", "cursor".into());
            set(&mut d, "action", args.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "status".into()).into());
            if args.len() > 2 { set(&mut d, "value", args[2].clone().into()); }
        }
        "usage" => { set(&mut d, "tool", "usage".into()); if args.len() > 1 { set(&mut d, "limit", int(&args[1])?.into()); } }
        "pause" | "resume" => set(&mut d, "tool", cmd.clone().into()),
        _ => return Err(format!("Unknown command `{cmd}`. Run `reach help`.")),
    }
    Ok(d)
}

fn take_list_flags(args: &[String], d: &mut Map<String, Value>) -> R<Vec<String>> {
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let has_next = i + 1 < args.len();
        match a {
            "--json" => { d.insert("json".into(), true.into()); }
            "--all" => { d.insert("all".into(), true.into()); }
            "--limit" if has_next => { i += 1; d.insert("limit".into(), int(&args[i])?.into()); }
            "--window" | "-w" if has_next => { i += 1; d.insert("window".into(), args[i].clone().into()); }
            _ => rest.push(args[i].clone()),
        }
        i += 1;
    }
    Ok(rest)
}

fn parse_flags(args: &[String], start: usize, d: &mut Map<String, Value>) -> R<()> {
    let mut i = start;
    while i < args.len() {
        let has_next = i + 1 < args.len();
        match args[i].as_str() {
            "--force" | "-f" => { d.insert("force".into(), true.into()); }
            "--include-overlay" => { d.insert("include_overlay".into(), true.into()); }
            "--max" if has_next => { i += 1; d.insert("max".into(), int(&args[i])?.into()); }
            "--quality" | "-q" if has_next => { i += 1; d.insert("quality".into(), int(&args[i])?.into()); }
            "--window" | "-w" if has_next => { i += 1; d.insert("window".into(), args[i].clone().into()); }
            "--region" | "-r" if has_next => {
                i += 1;
                let p: Vec<&str> = args[i].split([',', 'x']).filter(|s| !s.is_empty()).collect();
                if p.len() < 4 { return Err("Index was outside the bounds of the array.".into()); }
                d.insert("region".into(), Value::from(vec![int(p[0])?, int(p[1])?, int(p[2])?, int(p[3])?]));
            }
            _ => {}
        }
        i += 1;
    }
    Ok(())
}

/// Minimal HTTP/1.1 over loopback. Returns the body for any status.
fn http(method: &str, path: &str, body: Option<&str>, timeout_ms: u64) -> std::io::Result<(u16, String)> {
    let c = config::get();
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], c.port));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms.min(3000)))?;
    s.set_read_timeout(Some(Duration::from_millis(timeout_ms)))?;
    s.set_write_timeout(Some(Duration::from_millis(timeout_ms)))?;
    let b = body.unwrap_or("");
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n", c.port);
    if body.is_some() {
        req.push_str(&format!("Authorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n", c.token, b.len()));
    }
    req.push_str("\r\n");
    req.push_str(b);
    s.write_all(req.as_bytes())?;
    let mut raw = Vec::new();
    s.read_to_end(&mut raw)?;
    let text = String::from_utf8_lossy(&raw);
    let (head, rest) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let code = head.split_whitespace().nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    let chunked = head.to_ascii_lowercase().contains("transfer-encoding: chunked");
    Ok((code, if chunked { dechunk(rest) } else { rest.to_string() }))
}

fn dechunk(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some((len, tail)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
        if n == 0 || tail.len() < n { break; }
        out.push_str(&tail[..n]);
        rest = tail[n..].trim_start_matches("\r\n");
    }
    out
}

fn post(path: &str, body: &str) -> std::io::Result<String> {
    // Long enough for a pause-on-user wait plus the command itself.
    let t = 30000u64.max((config::get().user_pause_timeout_ms + 30000).max(0) as u64);
    http("POST", path, Some(body), t).map(|(_, b)| b)
}

pub fn health() -> Option<String> {
    match http("GET", "/health", None, 800) { Ok((200, b)) => Some(b), _ => None }
}

pub fn ensure_server() -> R<()> {
    use windows::Win32::System::Threading::{CreateProcessW, CREATE_NO_WINDOW, DETACHED_PROCESS, PROCESS_INFORMATION, STARTUPINFOW};
    if health().is_some() { return Ok(()); }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // No handle inheritance: the server must not hold the caller's stdout pipe open.
    let mut cmd = reach_core::util::wide(&format!("\"{}\" serve", exe.display()));
    let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut pi = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(None, Some(windows::core::PWSTR(cmd.as_mut_ptr())), None, None, false,
            CREATE_NO_WINDOW | DETACHED_PROCESS, None, None, &si, &mut pi).map_err(|e| e.to_string())?;
        let _ = windows::Win32::Foundation::CloseHandle(pi.hProcess);
        let _ = windows::Win32::Foundation::CloseHandle(pi.hThread);
    }
    for _ in 0..25 {
        std::thread::sleep(Duration::from_millis(150));
        if health().is_some() { return Ok(()); }
    }
    Err("Reach server did not start. Try `reach serve`.".into())
}

pub fn print_help() {
    println!(r#"reach {} - desktop control for AI agents

Apps and windows
  reach open notepad             launch, or focus if already open
  reach apps [filter] [--json]   one line per window: hwnd title [process] x,y wxh
  reach focus 0xHWND | close 0xHWND | displays

Screen
  reach screenshot [--max 1280] [--quality 72] [--region x,y,w,h] [--force]
  reach zoom X Y W H

Mouse and keyboard (screenshot image coordinates)
  reach click X Y | double-click X Y | right-click X Y | move X Y
  reach drag X1 Y1 X2 Y2 | scroll down 3 [X Y]
  reach type hello world | key ctrl+shift+t | key "ctrl+c, ctrl+v" | key win+r --hold 500
  reach motion smooth | fast | instant        how input moves (default smooth)

UI controls (preferred over pixels)
  reach ui find Save [--limit 25] [--json] [--all]
  reach ui inspect [--limit 60] [--json] [--all] [--window 0xHWND]
  reach ui invoke e3 | set e3 text | read e3 | toggle e3
  reach ui invoke Save           a name works too (resolved fresh)

Agent pill (name tag next to the real cursor while an agent acts)
  reach --agent "Claude" click X Y      or set REACH_AGENT
  reach cursor on | off | status | show [move|type|click] | ripple on|off
  --no-cursor                             skip it for one command
  reach screenshot --include-overlay      debug: capture with it visible

Pause on user
  reach pause-on-user on | off | status | idle <ms> | timeout <ms>
  --no-yield                              act now even if you are using the PC
  --fast                                  quick glides and bulk typing for this call
  --no-path                               don't add reach's folder to your PATH this run

Stop
  Ctrl+Alt+Esc                            stop the current action and pause Reach

Server
  reach path add | remove | status            reach's folder on your user PATH
  reach status [--json] | pause | resume | usage | serve | help"#, config::VERSION);
}

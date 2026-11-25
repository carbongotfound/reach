//! ~/.reach paths, token, config.json, usage.jsonl and reach.log. Same files and
//! formats as the C# build so both can share one home.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const DEFAULT_PORT: u16 = 7420;
pub const MAX_DIM: i32 = 1280;

pub fn home() -> PathBuf {
    match std::env::var("REACH_HOME") {
        Ok(h) if !h.is_empty() => PathBuf::from(h),
        _ => PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".into())).join(".reach"),
    }
}
pub fn isolated() -> bool {
    std::env::var("REACH_HOME").map(|h| !h.is_empty()).unwrap_or(false)
}
pub fn path(name: &str) -> PathBuf {
    home().join(name)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub port: u16,
    pub max_dim: i32,
    pub paused: bool,
    pub agent_cursor: bool,
    pub cursor_ripple: bool,
    pub pause_on_user: bool,
    pub user_pause_idle_ms: i64,
    pub user_pause_timeout_ms: i64,
    /// smooth | fast | instant (the app shows these as Human | Fast | Instant)
    pub motion: String,
    pub auto_path: bool,
    /// The app window's look: system | light | dark (the app only; Windows' setting is untouched)
    pub theme: String,
    /// The app's update check (app only): ask GitHub for new releases every 6 hours
    pub update_auto: bool,
    /// Offer pre-releases too
    pub update_beta: bool,
    /// Versions the user chose to skip
    pub update_skipped: Vec<String>,
    /// Unix seconds of the last answer from GitHub
    pub update_last_check: u64,
    /// "Remind me later": no card until this time (Unix seconds)
    pub update_later_until: u64,
    #[serde(skip)]
    pub token: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            port: DEFAULT_PORT,
            max_dim: MAX_DIM,
            paused: false,
            agent_cursor: true,
            cursor_ripple: true,
            pause_on_user: true,
            user_pause_idle_ms: 2000,
            user_pause_timeout_ms: 60000,
            motion: "smooth".into(),
            auto_path: true,
            theme: "system".into(),
            update_auto: true,
            update_beta: false,
            update_skipped: Vec::new(),
            update_last_check: 0,
            update_later_until: 0,
            token: String::new(),
        }
    }
}

fn cell() -> &'static Mutex<Config> {
    static C: OnceLock<Mutex<Config>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Config::default()))
}


/// Snapshot of the current config.
pub fn get() -> Config {
    cell().lock().unwrap().clone()
}

/// Change the config and save it.
pub fn update(f: impl FnOnce(&mut Config)) {
    let mut c = cell().lock().unwrap();
    f(&mut c);
    save(&c);
}

/// Replace the in-memory config without saving (used by the app for its private instance).
pub fn set(c: Config) {
    *cell().lock().unwrap() = c;
}

pub fn load() -> Config {
    let _ = fs::create_dir_all(home());
    let mut c = Config::default();
    let raw = fs::read_to_string(path("config.json")).ok();
    let mut need_save = raw.is_none();
    if let Some(raw) = raw {
        match serde_json::from_str::<serde_json::Value>(raw.trim_start_matches('\u{feff}')) {
            Ok(v) => {
                if let Ok(parsed) = serde_json::from_value::<Config>(v.clone()) {
                    c = parsed;
                }
                for k in ["port", "maxDim", "paused", "agentCursor", "cursorRipple", "pauseOnUser",
                          "userPauseIdleMs", "userPauseTimeoutMs", "motion", "autoPath"] {
                    if v.get(k).is_none() {
                        need_save = true;
                    }
                }
            }
            Err(_) => need_save = true,
        }
    }
    c.token = fs::read_to_string(path("token")).unwrap_or_default().trim().to_string();
    if c.token.is_empty() {
        c.token = crate::util::random_hex(32);
        let _ = fs::write(path("token"), &c.token);
    }
    if need_save {
        save(&c);
    }
    set(c.clone());
    c
}

fn save(c: &Config) {
    if let Ok(s) = serde_json::to_string(c) {
        let _ = fs::write(path("config.json"), s);
    }
}

pub fn log(s: &str) {
    let _ = fs::create_dir_all(home());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path("reach.log")) {
        let _ = f.write_all(format!("{} {}\n", crate::util::now_iso(), s).as_bytes());   // one write, no interleaving
    }
}

pub fn record_usage(tool: &str, action: &str, ms: u128, ok: bool, detail: &str) {
    let mut detail = detail.to_string();
    if detail.chars().count() > 120 {
        detail = detail.chars().take(117).collect::<String>() + "...";
    }
    let mut row = serde_json::Map::new();
    row.insert("ts".into(), crate::util::now_iso().into());
    row.insert("tool".into(), tool.into());
    row.insert("action".into(), action.into());
    row.insert("ms".into(), (ms as u64).into());
    row.insert("ok".into(), ok.into());
    if !detail.is_empty() {
        row.insert("detail".into(), detail.into());
    }
    let _ = fs::create_dir_all(home());
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path("usage.jsonl")) {
        let _ = f.write_all(format!("{}\n", serde_json::Value::Object(row)).as_bytes());
    }
}

pub fn usage_tail(n: usize) -> String {
    let Ok(all) = fs::read_to_string(path("usage.jsonl")) else { return "(no usage yet)".into() };
    let lines: Vec<&str> = all.lines().collect();
    let start = lines.len().saturating_sub(n);
    let mut s = format!("{} recorded call(s), last {}:\n", lines.len(), n.min(lines.len()));
    for l in &lines[start..] {
        s.push_str(l);
        s.push('\n');
    }
    s.trim_end().to_string()
}

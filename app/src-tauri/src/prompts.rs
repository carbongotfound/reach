//! Loads app/prompts/agent_system.md at run time (every task, so edits apply without a
//! rebuild) and splits it into the system prompt, the task wrapper and the app copy.

use crate::strings;
use reach_core::{apps, config, dpi, reg};
use std::collections::HashMap;
use std::path::PathBuf;

/// Shipped copy, used only when the file can't be found on disk.
const EMBEDDED: &str = include_str!("../../prompts/agent_system.md");

pub struct Prompts {
    pub system: String,
    pub wrapper: String,
    pub messages: HashMap<String, String>,
    pub source: String,
}

fn find_file() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("REACH_PROMPTS") { return Some(p.into()); }
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent();
    // Installed: <exe dir>/prompts. Dev/portable: walk up to the repo's app/prompts.
    let installed = dir?.join("prompts").join("agent_system.md");
    if installed.is_file() { return Some(installed); }
    while let Some(d) = dir {
        let p = d.join("app").join("prompts").join("agent_system.md");
        if p.is_file() { return Some(p); }
        dir = d.parent();
    }
    None
}

pub fn load() -> Prompts {
    let (text, source) = match find_file().and_then(|p| std::fs::read_to_string(&p).ok().map(|t| (t, p))) {
        Some((t, p)) => (t, p.display().to_string()),
        None => (EMBEDDED.to_string(), "embedded copy".into()),
    };
    parse(&text.replace("\r\n", "\n"), source)
}

fn parse(md: &str, source: String) -> Prompts {
    // The system prompt sits between the first two `---` lines.
    let mut parts = md.split("\n---\n");
    let _intro = parts.next();
    let system = parts.next().unwrap_or("").trim().to_string();
    let rest = parts.collect::<Vec<_>>().join("\n---\n");

    let wrapper = rest.split("## Task wrapper").nth(1)
        .and_then(|s| s.split("```").nth(1))
        .map(|s| s.trim_start_matches(|c: char| c != '\n').trim_start_matches('\n').trim_end().to_string())
        .unwrap_or_else(|| "Task from the user:\n<task>\n{{user_text}}\n</task>".into());

    // App copy: `- Label: "text"` bullets under "Error and status messages".
    let mut messages = HashMap::new();
    if let Some(sec) = rest.split("## Error and status messages").nth(1) {
        for line in sec.split("\n## ").next().unwrap_or("").lines() {
            let Some(l) = line.strip_prefix("- ") else { continue };
            let Some((label, v)) = l.split_once(": \"") else { continue };
            if let Some(end) = v.find('"') { messages.insert(label.trim().to_string(), v[..end].to_string()); }
        }
    }
    Prompts { system, wrapper, messages, source }
}

impl Prompts {
    /// App copy by label. Labels the file doesn't have fall back to the same plain voice.
    pub fn msg(&self, label: &str) -> String {
        if let Some(m) = self.messages.get(label) { return m.clone(); }
        match label {
            "Not installed" => strings::FALLBACK_NOT_INSTALLED,
            "Brain crashed" => strings::FALLBACK_BRAIN_CRASHED,
            _ => label,
        }.to_string()
    }
}

/// Replaces {{name}}; drops {{#if name}}...{{/if}} blocks whose value is empty.
pub fn fill(t: &str, vars: &[(&str, String)]) -> String {
    let mut s = t.to_string();
    for (k, v) in vars {
        let open = format!("{{{{#if {k}}}}}");
        while let Some(i) = s.find(&open) {
            let Some(j) = s[i..].find("{{/if}}").map(|j| i + j) else { break };
            let inner = s[i + open.len()..j].to_string();
            s.replace_range(i..j + 7, if v.is_empty() { "" } else { &inner });
        }
        s = s.replace(&format!("{{{{{k}}}}}"), v);
    }
    s
}

fn windows_version() -> String {
    let k = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let build: u32 = reg::read(reg::HKLM, k, "CurrentBuild").and_then(|v| v.0.parse().ok()).unwrap_or(0);
    let disp = reg::read(reg::HKLM, k, "DisplayVersion").map(|v| v.0).unwrap_or_default();
    format!("Windows {} {} (build {build})", if build >= 22000 { 11 } else { 10 }, disp).replace("  ", " ")
}

fn now_local() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    let day = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"][t.wDayOfWeek as usize % 7];
    format!("{day} {:04}-{:02}-{:02} {:02}:{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute)
}

fn installed(exe: &str) -> bool {
    let k = format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{exe}");
    reg::read(reg::HKLM, &k, "").is_some() || reg::read(reg::HKCU, &k, "").is_some()
}

/// The filled system prompt for one run. Open apps are listed by app name only
/// (no window titles), so private titles don't leave the machine in the prompt.
pub fn system_for(p: &Prompts, agent: &str) -> String {
    let ds = dpi::displays();
    let screens = ds.iter().map(|d| format!("{}x{}{}", d.bounds.w, d.bounds.h, if d.primary { " primary" } else { "" })).collect::<Vec<_>>().join(", ");
    let scales = ds.iter().map(|d| format!("{}%", d.dpi * 100 / 96)).collect::<Vec<_>>().join(", ");
    let me = std::process::id();
    let mut open: Vec<String> = Vec::new();
    for w in apps::enumerate() {
        if w.pid == me || w.process.is_empty() { continue; }
        let n = w.process.trim_end_matches(".exe").to_string();
        if !open.iter().any(|o| o.eq_ignore_ascii_case(&n)) { open.push(n); }
    }
    let mut rel: Vec<&str> = [("excel.exe", "Microsoft Excel"), ("winword.exe", "Microsoft Word"), ("outlook.exe", "Microsoft Outlook"), ("powerpnt.exe", "Microsoft PowerPoint")]
        .iter().filter(|(e, _)| installed(e)).map(|(_, n)| *n).collect();
    rel.push("Notepad");
    let speed = match config::get().motion.as_str() { "instant" => "Instant", "smooth" => "Human", _ => "Fast" };
    fill(&p.system, &[
        ("agent_name", agent.to_string()),
        ("datetime", now_local()),
        ("windows_version", windows_version()),
        ("display_summary", format!("{} screen{}: {screens}", ds.len(), if ds.len() == 1 { "" } else { "s" })),
        ("dpi_scale", scales),
        ("open_apps", if open.is_empty() { "none".into() } else { open.join(", ") }),
        ("speed_mode", speed.into()),
        ("relevant_apps", rel.join(", ")),
    ])
}

pub fn task_for(p: &Prompts, user_text: &str) -> String {
    fill(&p.wrapper, &[("user_text", user_text.trim().to_string()), ("attachments", String::new())])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_the_real_file() {
        let p = parse(&EMBEDDED.replace("\r\n", "\n"), "t".into());
        assert!(p.system.starts_with("You are {{agent_name}}, an assistant"));
        assert!(p.system.ends_with("{{relevant_apps}}"));
        assert!(p.wrapper.starts_with("Task from the user:"));
        assert!(p.wrapper.ends_with("ask one question before you begin."));
        assert_eq!(p.messages["Stopped"], "Stopped. Nothing was sent or deleted after this point.");
        assert!(p.messages["Not signed in"].starts_with("Claude isn't signed in"));
        assert_eq!(p.messages.len(), 5);
        let t = task_for(&p, "open Notepad");
        assert!(t.contains("<task>\nopen Notepad\n</task>") && !t.contains("{{") && !t.contains("Attached"));
        assert!(!system_for(&p, "Claude").contains("{{"));
    }
}

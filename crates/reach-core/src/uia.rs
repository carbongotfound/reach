//! UI Automation through IUIAutomation: find / inspect with one cached bulk fetch,
//! stable refs (eN) keyed by runtime id, and invoke / set / read / toggle / focus.

use crate::dpi::{Rect, ScreenMap};
use crate::util::{clip, quote, Args};
use crate::{emit_pointer, input, yield_};
use std::collections::HashMap;
use windows::core::{Interface, BSTR};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
use windows::Win32::System::Ole::{SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

type R<T> = Result<T, String>;

pub struct Item {
    pub el: IUIAutomationElement,
    pub ty: String,
    pub name: String,
    pub value: String,
    pub aid: String,
    pub rect: Rect,
    pub enabled: bool,
    pub focus: bool,
    pub interactive: bool,
    pub offscreen: bool,
    pub toggle: Option<&'static str>,
    pub score: i32,
}

pub struct Uia {
    pub auto: IUIAutomation,
    cache: IUIAutomationCacheRequest,
    cache_kids: IUIAutomationCacheRequest,
    view: IUIAutomationCondition,
    refs: HashMap<String, IUIAutomationElement>,
    by_runtime: HashMap<String, String>,
    next: u32,
}

const PROPS: &[UIA_PROPERTY_ID] = &[
    UIA_NamePropertyId, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId, UIA_BoundingRectanglePropertyId,
    UIA_IsEnabledPropertyId, UIA_AutomationIdPropertyId, UIA_HasKeyboardFocusPropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsInvokePatternAvailablePropertyId, UIA_IsValuePatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_ValueValuePropertyId, UIA_ToggleToggleStatePropertyId, UIA_ProcessIdPropertyId,
];

const TYPES: &[&str] = &["button", "calendar", "checkbox", "combobox", "edit", "hyperlink", "image", "listitem", "list", "menu",
    "menubar", "menuitem", "progressbar", "radiobutton", "scrollbar", "slider", "spinner", "statusbar", "tab", "tabitem", "text",
    "toolbar", "tooltip", "tree", "treeitem", "custom", "group", "thumb", "datagrid", "dataitem", "document", "splitbutton",
    "window", "pane", "header", "headeritem", "table", "titlebar", "separator", "semanticzoom", "appbar"];

const INTERACTIVE: &[&str] = &["button", "checkbox", "combobox", "edit", "hyperlink", "listitem", "menuitem", "radiobutton",
    "slider", "spinner", "tabitem", "treeitem", "splitbutton", "dataitem", "headeritem", "document", "menubar"];
const UNNAMED_OK: &[&str] = &["edit", "document", "combobox", "checkbox", "radiobutton", "slider", "spinner"];
const NOISE: &[&str] = &["pane", "window", "titlebar", "scrollbar", "thumb", "separator"];

/// Process whose UIA Invoke hasn't returned yet (a modal dialog it opened is up), or 0.
static PENDING_PID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const BUSY: &str = "UI Automation can't reach that app until the dialog closes; use the keyboard there (Tab, typing, Enter), then screenshot to check.";

fn e<T>(r: windows::core::Result<T>) -> R<T> { r.map_err(|x| x.message().to_string()) }

fn cached_bool(el: &IUIAutomationElement, p: UIA_PROPERTY_ID) -> bool {
    unsafe { el.GetCachedPropertyValue(p).ok().and_then(|v| bool::try_from(&v).ok()).unwrap_or(false) }
}
fn cached_str(el: &IUIAutomationElement, p: UIA_PROPERTY_ID) -> String {
    unsafe { el.GetCachedPropertyValue(p).ok().and_then(|v| BSTR::try_from(&v).ok()).map(|b| b.to_string()).unwrap_or_default() }
}
fn cached_i32(el: &IUIAutomationElement, p: UIA_PROPERTY_ID) -> Option<i32> {
    unsafe { el.GetCachedPropertyValue(p).ok().and_then(|v: VARIANT| i32::try_from(&v).ok()) }
}

impl Uia {
    pub fn new() -> R<Uia> {
        unsafe {
            let auto: IUIAutomation = e(CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER))
                .or_else(|_| e(CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)))?;
            // A hung app can't block Reach forever.
            if let Ok(a2) = auto.cast::<IUIAutomation2>() {
                let _ = a2.SetConnectionTimeout(3000);
                let _ = a2.SetTransactionTimeout(8000);
            }
            let view = e(auto.ControlViewCondition())?;
            let cache = e(auto.CreateCacheRequest())?;
            for p in PROPS { let _ = cache.AddProperty(*p); }
            e(cache.SetTreeScope(TreeScope_Element))?;
            e(cache.SetTreeFilter(&view))?;
            let cache_kids = e(cache.Clone())?;
            e(cache_kids.SetTreeScope(TreeScope(TreeScope_Element.0 | TreeScope_Children.0)))?;
            Ok(Uia { auto, cache, cache_kids, view, refs: HashMap::new(), by_runtime: HashMap::new(), next: 1 })
        }
    }

    fn to_item(&self, el: IUIAutomationElement) -> Item {
        unsafe {
            let ct = el.CachedControlType().map(|c| c.0).unwrap_or(0);
            let ty = TYPES.get((ct - 50000) as usize).filter(|_| ct >= 50000).map(|s| s.to_string()).unwrap_or_else(|| "?".into());
            let r = el.CachedBoundingRectangle().unwrap_or_default();
            let pat = [UIA_IsInvokePatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId, UIA_IsSelectionItemPatternAvailablePropertyId,
                       UIA_IsExpandCollapsePatternAvailablePropertyId, UIA_IsValuePatternAvailablePropertyId].iter().any(|p| cached_bool(&el, *p));
            let toggle = if cached_bool(&el, UIA_IsTogglePatternAvailablePropertyId) {
                cached_i32(&el, UIA_ToggleToggleStatePropertyId).map(|t| match t { 1 => "on", 0 => "off", _ => "mixed" })
            } else { None };
            Item {
                interactive: INTERACTIVE.contains(&ty.as_str()) || pat,
                name: el.CachedName().map(|b| b.to_string()).unwrap_or_default(),
                value: cached_str(&el, UIA_ValueValuePropertyId),
                aid: el.CachedAutomationId().map(|b| b.to_string()).unwrap_or_default(),
                rect: Rect::from_ltrb(r.left, r.top, r.right, r.bottom),
                offscreen: el.CachedIsOffscreen().map(|b| b.as_bool()).unwrap_or(false),
                enabled: el.CachedIsEnabled().map(|b| b.as_bool()).unwrap_or(false),
                focus: el.CachedHasKeyboardFocus().map(|b| b.as_bool()).unwrap_or(false),
                toggle, ty, el, score: 0,
            }
        }
    }

    fn useful(it: &Item) -> bool {
        if it.offscreen || it.rect.w < 1 || it.rect.h < 1 || it.ty == "titlebar" { return false; }
        if it.interactive { return !it.name.trim().is_empty() || !it.value.is_empty() || UNNAMED_OK.contains(&it.ty.as_str()); }
        !it.name.trim().is_empty() && !NOISE.contains(&it.ty.as_str())
    }

    fn contains(a: &Rect, b: &Rect) -> bool { b.x >= a.x && b.y >= a.y && b.right() <= a.right() && b.bottom() <= a.bottom() }

    pub fn root_for(&self, a: &Args) -> R<IUIAutomationElement> {
        let w = crate::util::s(a, "window", "");
        unsafe {
            // An invoke into this app is still waiting on a dialog it opened: UIA calls to the app
            // hang (3.2 s each in tests), so fail fast and point at the keyboard.
            let pending = PENDING_PID.load(std::sync::atomic::Ordering::SeqCst);
            if pending != 0 {
                let h = if w.is_empty() { GetForegroundWindow() } else { HWND(crate::apps::resolve(&w)? as *mut _) };
                let mut pid = 0u32;
                windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(h, Some(&mut pid));
                if pid == pending { return Err(BUSY.into()); }
            }
            if !w.is_empty() {
                let h = crate::apps::resolve(&w)?;
                return self.auto.ElementFromHandle(HWND(h as *mut _)).map_err(|_| "No automation tree for that window.".to_string());
            }
            let fg = GetForegroundWindow();
            if !fg.0.is_null() {
                if let Ok(el) = self.auto.ElementFromHandle(fg) { return Ok(el); }
            }
            e(self.auto.GetRootElement())
        }
    }

    /// query None: inspect-style filtering. all: no filtering.
    pub fn collect(&self, root: &IUIAutomationElement, query: Option<&str>, all: bool) -> R<Vec<Item>> {
        let arr = unsafe { e(root.FindAllBuildCache(TreeScope_Subtree, &self.view, &self.cache))? };
        let n = unsafe { arr.Length().unwrap_or(0) };
        let q = query.map(|q| q.to_lowercase());
        let mut out: Vec<Item> = Vec::new();
        for i in 0..n {
            let Ok(el) = (unsafe { arr.GetElement(i) }) else { continue };
            let mut it = self.to_item(el);
            if all { if !it.offscreen { out.push(it); } continue; }
            if let Some(q) = &q {
                if it.offscreen { continue; }
                let nm = it.name.to_lowercase();
                let hit = nm.contains(q.as_str()) || it.value.to_lowercase().contains(q.as_str()) || it.aid.to_lowercase() == *q || it.ty == *q;
                if !hit { continue; }
                it.score = (if nm == *q { 0 } else if nm.starts_with(q.as_str()) { 1 } else { 2 }) * 2 + if it.interactive { 0 } else { 1 };
                out.push(it);
                continue;
            }
            if !Self::useful(&it) { continue; }
            if let Some(last) = out.last() {
                if !it.interactive && it.ty == "text" && it.name == last.name && Self::contains(&last.rect, &it.rect) { continue; }
            }
            out.push(it);
        }
        if q.is_some() { out.sort_by_key(|i| i.score); } // stable
        Ok(out)
    }

    /// Depth-first walk, one cached round trip per node, stops at `limit` useful controls.
    pub fn walk(&self, root: &IUIAutomationElement, all: bool, limit: usize, max_depth: usize) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::new();
        let mut stack = vec![(root.clone(), 0usize)];
        while let Some((el, depth)) = stack.pop() {
            if out.len() > limit { break; }
            let Ok(el) = (unsafe { el.BuildUpdatedCache(&self.cache_kids) }) else { continue };
            let kids = unsafe { el.GetCachedChildren().ok() };
            let it = self.to_item(el);
            let mut keep = if all { !it.offscreen } else { Self::useful(&it) };
            if keep && !all {
                if let Some(last) = out.last() {
                    if !it.interactive && it.ty == "text" && it.name == last.name && Self::contains(&last.rect, &it.rect) { keep = false; }
                }
            }
            if keep { out.push(it); }
            if depth >= max_depth { continue; }
            if let Some(kids) = kids {
                let n = unsafe { kids.Length().unwrap_or(0) };
                for i in (0..n).rev() {
                    if let Ok(k) = unsafe { kids.GetElement(i) } { stack.push((k, depth + 1)); }
                }
            }
        }
        out
    }

    pub fn runtime_key(el: &IUIAutomationElement) -> Option<String> {
        unsafe {
            let sa = el.GetRuntimeId().ok()?;
            if sa.is_null() { return None; }
            let (lo, hi) = (SafeArrayGetLBound(sa, 1).ok()?, SafeArrayGetUBound(sa, 1).ok()?);
            let mut parts = Vec::new();
            for i in lo..=hi {
                let mut v = 0i32;
                let _ = SafeArrayGetElement(sa, &i, &mut v as *mut _ as *mut _);
                parts.push(v.to_string());
            }
            let _ = SafeArrayDestroy(sa);
            // Some proxies (Win32 list items) return an empty id: no stable key.
            if parts.is_empty() { None } else { Some(parts.join(".")) }
        }
    }

    pub fn add_ref(&mut self, el: &IUIAutomationElement) -> String {
        let key = Self::runtime_key(el);
        if let Some(k) = &key {
            if let Some(id) = self.by_runtime.get(k) {
                self.refs.insert(id.clone(), el.clone());
                return id.clone();
            }
        }
        if self.refs.len() > 3000 { self.refs.clear(); self.by_runtime.clear(); self.next = 1; }
        let id = format!("e{}", self.next);
        self.next += 1;
        self.refs.insert(id.clone(), el.clone());
        if let Some(k) = key { self.by_runtime.insert(k, id.clone()); }
        id
    }

    pub fn is_ref(s: &str) -> bool {
        s.len() >= 2 && (s.starts_with('e') || s.starts_with('E')) && s[1..].bytes().all(|b| b.is_ascii_digit())
    }

    pub fn get(&mut self, id: &str, a: &Args) -> R<IUIAutomationElement> {
        if id.is_empty() { return Err("`ref` is required (eN from ui find, or a control name).".into()); }
        if Self::is_ref(id) {
            let el = self.refs.get(&id.to_lowercase()).cloned().ok_or_else(|| format!("Unknown ref `{id}`. Run ui find or ui inspect first."))?;
            if unsafe { el.CurrentProcessId() }.is_err() {
                return Err(format!("Ref `{id}` is gone (the control closed or changed). Run ui find again."));
            }
            return Ok(el);
        }
        let root = self.root_for(a)?;
        let hits = self.collect(&root, Some(id), false)?;
        let first = hits.into_iter().next().ok_or_else(|| format!("No control matching `{id}`."))?;
        self.add_ref(&first.el);
        Ok(first.el)
    }

    pub fn format(&mut self, items: Vec<Item>, limit: usize, json: bool, empty: &str) -> String {
        if items.is_empty() { return if json { "[]".into() } else { empty.into() }; }
        let map = ScreenMap::build(crate::config::get().max_dim);
        let shown = limit.min(items.len());
        let total = items.len();
        if json {
            let mut list = Vec::new();
            for it in items.iter().take(shown) {
                let (x0, y0) = map.to_image(it.rect.x, it.rect.y);
                let (x1, y1) = map.to_image(it.rect.right(), it.rect.bottom());
                let mut d = serde_json::Map::new();
                d.insert("ref".into(), self.add_ref(&it.el).into());
                d.insert("type".into(), it.ty.clone().into());
                d.insert("name".into(), it.name.clone().into());
                if !it.value.is_empty() { d.insert("value".into(), it.value.clone().into()); }
                if !it.aid.is_empty() { d.insert("aid".into(), it.aid.clone().into()); }
                for (k, v) in [("x", x0), ("y", y0), ("w", (x1 - x0).max(0)), ("h", (y1 - y0).max(0)), ("cx", (x0 + x1) / 2), ("cy", (y0 + y1) / 2)] {
                    d.insert(k.into(), v.into());
                }
                d.insert("enabled".into(), it.enabled.into());
                if it.focus { d.insert("focused".into(), true.into()); }
                if let Some(t) = it.toggle { d.insert("toggle".into(), t.into()); }
                d.insert("interactive".into(), it.interactive.into());
                list.push(serde_json::Value::Object(d));
            }
            return serde_json::json!({ "items": list, "total": total }).to_string();
        }
        let mut s = String::new();
        for it in items.iter().take(shown) {
            let (x0, y0) = map.to_image(it.rect.x, it.rect.y);
            let (x1, y1) = map.to_image(it.rect.right(), it.rect.bottom());
            s.push_str(&self.add_ref(&it.el));
            s.push(' ');
            s.push_str(&it.ty);
            if !it.name.is_empty() { s.push(' '); s.push_str(&quote(&clip(&it.name, 60))); }
            if !it.value.is_empty() && it.value != it.name { s.push_str(" ="); s.push_str(&quote(&clip(&it.value, 40))); }
            s.push_str(&format!(" {},{}", (x0 + x1) / 2, (y0 + y1) / 2));
            if let Some(t) = it.toggle { s.push(' '); s.push_str(t); }
            if !it.enabled { s.push_str(" disabled"); }
            if it.focus { s.push_str(" focused"); }
            s.push('\n');
        }
        if total > shown { s.push_str(&format!("+{} more (--limit)", total - shown)); }
        s.trim_end().to_string()
    }

    /// Center of the element in screen px, for the overlay cursor.
    pub fn center(el: &IUIAutomationElement) -> Option<(i32, i32)> {
        let r = unsafe { el.CurrentBoundingRectangle().ok()? };
        if r.right <= r.left { return None; }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    }

    fn point_at(el: &IUIAutomationElement, kind: &str) {
        if let Some((x, y)) = Self::center(el) {
            // "click:Save" lets the pill say "Clicking Save"
            let name = unsafe { el.CurrentName().map(|s| s.to_string()).unwrap_or_default() };
            let name = name.trim();
            let kind = if name.is_empty() || kind == "move" || name.chars().count() > 40 { kind.to_string() } else { format!("{kind}:{name}") };
            emit_pointer(x, y, &kind, false);
        }
    }

    pub fn invoke(&mut self, id: &str, a: &Args) -> R<String> {
        let el = self.get(id, a)?;
        crate::guard::check_el(&el)?;
        Self::point_at(&el, "click");
        unsafe {
            let _ = el.SetFocus();
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) {
                // Invoke blocks while a modal dialog it opened is up (seen: 8.6 s for File > Save).
                // Run it on its own MTA thread and stop waiting after 1.5 s.
                struct Agile(IUIAutomationInvokePattern);
                unsafe impl Send for Agile {}
                let (tx, rx) = std::sync::mpsc::channel();
                let job = Agile(p);
                let pid = el.CurrentProcessId().unwrap_or(0) as u32;
                std::thread::spawn(move || {
                    let job = job;
                    let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
                    let r = e(job.0.Invoke());
                    let _ = PENDING_PID.compare_exchange(pid, 0, std::sync::atomic::Ordering::SeqCst, std::sync::atomic::Ordering::SeqCst);
                    let _ = tx.send(r);
                });
                return match rx.recv_timeout(std::time::Duration::from_millis(1500)) {
                    Ok(r) => r.map(|_| format!("invoked {id}")),
                    Err(_) => {
                        PENDING_PID.store(pid, std::sync::atomic::Ordering::SeqCst);
                        Ok(format!("invoked {id}. The app is still busy, probably showing a dialog. {BUSY}"))
                    }
                };
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) {
                e(p.Toggle())?;
                return Ok(format!("toggled {id}"));
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId) {
                e(p.Select())?;
                return Ok(format!("selected {id}"));
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId) {
                if p.CurrentExpandCollapseState().map(|s| s == ExpandCollapseState_Expanded).unwrap_or(false) { e(p.Collapse())?; } else { e(p.Expand())?; }
                return Ok(format!("expanded/collapsed {id}"));
            }
        }
        // No pattern: real click at its center. That moves the mouse, so wait for the user first.
        if let Some((x, y)) = Self::center(&el) {
            if !crate::ctx().no_yield { yield_::wait_for_idle(&crate::ctx().agent)?; }
            let map = ScreenMap::build(crate::config::get().max_dim);
            let (ix, iy) = map.to_image(x, y);
            input::click_at(&map, ix as i64, iy as i64, "left", 1)?;
            return Ok(format!("clicked {id} (no invoke pattern)"));
        }
        Err(format!("Could not invoke {id}."))
    }

    pub fn set_value(&mut self, id: &str, value: &str, a: &Args) -> R<String> {
        let el = self.get(id, a)?;
        Self::point_at(&el, "type");
        unsafe {
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                // Windows edit controls need CRLF; a lone LF is dropped (the agent's tables lost their rows).
                let v = if value.contains('\n') && !value.contains("\r\n") { value.replace('\n', "\r\n") } else { value.to_string() };
                if p.SetValue(&BSTR::from(v.as_str())).is_ok() {
                    return Ok(format!("set {id}"));
                }
            }
            let _ = el.SetFocus();
        }
        if !crate::ctx().no_yield { yield_::wait_for_idle(&crate::ctx().agent)?; }
        std::thread::sleep(std::time::Duration::from_millis(50));
        input::press_chord("ctrl+a", 0)?;
        std::thread::sleep(std::time::Duration::from_millis(30));
        input::type_text(value)?;
        Ok(format!("typed into {id}"))
    }

    pub fn toggle(&mut self, id: &str, a: &Args) -> R<String> {
        let el = self.get(id, a)?;
        Self::point_at(&el, "click");
        unsafe {
            let p = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId).map_err(|_| "That control has no toggle pattern.".to_string())?;
            e(p.Toggle())?;
        }
        Ok("toggled".into())
    }

    pub fn focus(&mut self, id: &str, a: &Args) -> R<String> {
        let el = self.get(id, a)?;
        Self::point_at(&el, "move");
        unsafe { let _ = el.SetFocus(); }
        Ok("ok".into())
    }

    pub fn read(&mut self, id: &str, a: &Args) -> R<String> {
        let el = self.get(id, a)?;
        let mut s = String::new();
        unsafe {
            if let (Ok(t), Ok(n)) = (el.CurrentLocalizedControlType(), el.CurrentName()) {
                s.push_str(&format!("[{}] {}", t, quote(&n.to_string())));
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
                if let Ok(v) = p.CurrentValue() { s.push_str(&format!("  value={}", quote(&v.to_string()))); }
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) {
                if let Ok(t) = p.DocumentRange().and_then(|r| r.GetText(2000)) {
                    let t = t.to_string();
                    if !t.is_empty() { s.push_str(&format!("  text={}", quote(&t))); }
                }
            }
            if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) {
                if let Ok(t) = p.CurrentToggleState() {
                    s.push_str(match t.0 { 1 => "  toggle=On", 0 => "  toggle=Off", _ => "  toggle=Indeterminate" });
                }
            }
        }
        Ok(s.trim().to_string())
    }
}

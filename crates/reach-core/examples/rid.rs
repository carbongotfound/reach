// Debug: print runtime-id keys and `ui read` for controls in a window. cargo run --example rid -- 0xHWND
fn main() {
    unsafe { let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED); }
    let mut u = reach_core::uia::Uia::new().unwrap();
    let mut a = serde_json::Map::new();
    a.insert("window".into(), std::env::args().nth(1).expect("hwnd").into());
    let root = u.root_for(&a).unwrap();
    for it in u.walk(&root, false, 60, 25) {
        let id = u.add_ref(&it.el);
        println!("{id} {} {:?} {:?} read={:?}", it.ty, it.name, reach_core::uia::Uia::runtime_key(&it.el), u.read(&id, &a));
        unsafe {
            use windows::Win32::UI::Accessibility::*;
            if it.ty == "edit" {
                let p = it.el.GetCurrentPattern(UIA_TextPatternId);
                println!("   text pattern raw: {:?}", p.is_ok());
            }
        }
    }
}


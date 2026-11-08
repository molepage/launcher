//! Launcher core: everything except drawing the window.
//!
//! The shell talks to it through the small C ABI below. Strings in and out
//! are UTF-8 and NUL-terminated; results are JSON. Free every returned
//! string with `lc_free`.

pub mod addons;
pub mod calc;
pub mod clips;
pub mod config;
pub mod engine;
pub mod fileindex;
pub mod item;
pub mod learn;
pub mod matcher;
pub mod runner;
pub mod sources;
pub mod syslinks;

use engine::{Engine, Options};
use std::ffi::{CStr, CString, c_char};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;

static ENGINE: OnceLock<Engine> = OnceLock::new();

unsafe fn arg(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

fn out(s: String) -> *mut c_char {
    CString::new(s).map(CString::into_raw).unwrap_or(std::ptr::null_mut())
}

fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(default)
}

fn with_engine<T>(default: T, f: impl FnOnce(&Engine) -> T) -> T {
    match ENGINE.get() {
        Some(e) => guard(default, || f(e)),
        None => default,
    }
}

/// Start the core. `options_json`: {"data_dir", "home", "screenshot_dir"?,
/// "os_major"?}.
/// Returns NULL on success, or an error message (free it).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_init(options_json: *const c_char) -> *mut c_char {
    let json = unsafe { arg(options_json) };
    guard(out("panic during init".into()), || {
        let opts: Options = match serde_json::from_str(&json) {
            Ok(o) => o,
            Err(e) => return out(format!("bad options: {e}")),
        };
        match Engine::open(opts) {
            Ok(e) => {
                let _ = ENGINE.set(e);
                std::ptr::null_mut()
            }
            Err(e) => out(e),
        }
    })
}

/// Fast results for the text typed so far: {"items": [...], "slow": bool}.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_query(query: *const c_char) -> *mut c_char {
    let q = unsafe { arg(query) };
    with_engine(std::ptr::null_mut(), |e| out(serde_json::to_string(&e.query(&q)).unwrap_or_default()))
}

/// Slow results (files, add-ons) as {"items": [...]}. Blocks; call off the
/// main thread. Returns NULL when superseded or nothing applies.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_query_slow(query: *const c_char) -> *mut c_char {
    let q = unsafe { arg(query) };
    with_engine(std::ptr::null_mut(), |e| {
        out(serde_json::json!({ "items": e.query_slow(&q) }).to_string())
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn lc_cancel_slow() {
    with_engine((), |e| e.cancel_slow())
}

/// Record that `uid` was chosen for `query` (feeds the ranking).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_picked(query: *const c_char, uid: *const c_char) {
    let (q, u) = unsafe { (arg(query), arg(uid)) };
    with_engine((), |e| e.picked(&q, &u))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_clipboard_add(text: *const c_char, app: *const c_char) {
    let (t, a) = unsafe { (arg(text), arg(app)) };
    with_engine((), |e| e.clipboard_add(&t, &a))
}

/// Rescan apps, config and add-ons (throttled).
#[unsafe(no_mangle)]
pub extern "C" fn lc_refresh() {
    with_engine((), |e| e.refresh(false))
}

/// Anchors and titles of the built-in System Settings entries visible on
/// this OS: [{"anchor", "title"}, ...]. The settings window's enable-all
/// toggle needs the full list.
#[unsafe(no_mangle)]
pub extern "C" fn lc_system_links() -> *mut c_char {
    with_engine(std::ptr::null_mut(), |e| {
        let links: Vec<_> = e.system_link_anchors()
            .into_iter()
            .map(|(anchor, title)| serde_json::json!({ "anchor": anchor, "title": title }))
            .collect();
        out(serde_json::to_string(&links).unwrap_or_default())
    })
}

/// {"hotkey", "clipboard_hotkey", "app_dirs", "appearance", "errors": [...],
/// "data_dir"}.
#[unsafe(no_mangle)]
pub extern "C" fn lc_settings() -> *mut c_char {
    with_engine(std::ptr::null_mut(), |e| out(e.settings().to_string()))
}

/// Merge `config_json` fields into config.toml and reload. Returns the new
/// settings JSON, or {"error": "..."}.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_write_config(config_json: *const c_char) -> *mut c_char {
    let json = unsafe { arg(config_json) };
    with_engine(std::ptr::null_mut(), |e| {
        match serde_json::from_str::<serde_json::Value>(&json) {
            Ok(fields) => out(e.write_config(&fields).to_string()),
            Err(err) => out(serde_json::json!({ "error": format!("bad config json: {err}") }).to_string()),
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn lc_free(s: *mut c_char) {
    if !s.is_null() {
        drop(unsafe { CString::from_raw(s) });
    }
}

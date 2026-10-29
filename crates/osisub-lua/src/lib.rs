//! osisub-lua: Aegisub Automation 4 (Lua) uyumlu betik motoru.
//!
//! `aegisub.*` API'si, `subs` nesnesi, `karaskel.lua`, `utils.lua`, `unicode`, `re`, `lfs`
//! modülleri bulunur; mevcut Aegisub Lua betikleri değişiklik gerekmeden çalışır.

mod api;
mod dialog;
mod engine;
mod subs;

pub use dialog::{Control, DialogResult, DialogSpec, DV};
pub use engine::{Engine, EngineError, Host, HeadlessHost, MacroInfo, MacroKind, RunOutcome, ScriptInfo, StyleMetrics};

//! osisub-lua: Aegisub Automation 4 (Lua) uyumlu betik motoru.
//!
//! `aegisub.*` API'si, `subs` nesnesi, `karaskel.lua`, `utils.lua`, `unicode`, `re`, `lfs`
//! modülleri bulunur; mevcut Aegisub Lua betikleri değişiklik gerekmeden çalışır.

mod api;
mod dialog;
mod engine;
pub mod fonts;
mod subs;
mod vendor;

pub use dialog::{Control, DialogResult, DialogSpec, DV};
pub use engine::{builtin_script, BUILTIN_PREFIX, BUILTIN_SCRIPTS, Engine, EngineError, Host, HeadlessHost, MacroInfo, MacroKind, RunOutcome, ScriptInfo, StyleMetrics};

//! Aegisub Automation 4 compatible Lua runtime for OsioSub.
//!
//! Implements the `aegisub.*` API, the `subs` object, `include()`, and bundled
//! `util.lua` / `karaskel.lua` / `unicode` / `re` / `utf8` modules so existing
//! Aegisub macros run unchanged.
pub mod fonts;
mod host;
mod lines;
mod modules;
mod runtime;

pub use host::{DefaultHost, DialogControl, DialogResult, DialogSpec, DlgVal, Host};
pub use runtime::{MacroInfo, MacroResult, ScriptRuntime};

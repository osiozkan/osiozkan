//! osisub-core: subtitle data model, parsers/writers, checks and editing operations.
pub mod ass;
pub mod checks;
pub mod ops;
pub mod srt;
pub mod tags;
pub mod time;

pub use ass::{Entry, Event, Script, Style};

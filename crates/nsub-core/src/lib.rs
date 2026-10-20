//! nsub-core: subtitle document model, formats, timing and QC tools.

pub mod ass;
pub mod metrics;
pub mod color;
pub mod fixes;
pub mod formats;
pub mod kara;
pub mod ops;
pub mod qc;
pub mod tags;
pub mod text;
pub mod time;

pub use ass::{Event, Script, Style};
pub use color::Color;

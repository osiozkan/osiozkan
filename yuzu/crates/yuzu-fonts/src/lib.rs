//! Aegisub's "Fonts Collector" and "Attachments" for Yuzu.
//!
//! * [`used_fonts`] finds every font a script really uses (styles + `\fn`, `\b`, `\i`, `\r`).
//! * [`locate`] maps a [`FontUse`] to a font file through a `fontdb` database.
//! * [`collect`] copies the files into a folder and/or a `.zip`.
//! * [`attach`], [`embed`], [`extract_embedded`], [`remove_embedded`] handle the `[Fonts]`
//!   and `[Graphics]` sections with Aegisub's uuencode-style encoding.

mod attach;
mod collect;
mod locate;
mod used;

pub use attach::{
    attach, attachments, decode, embed, embed_graphic, encode, extract_embedded, extract_graphics, remove_attachment,
    remove_embedded, AttachKind,
};
pub use collect::{collect, collect_located, Entry, Options, Report, Status};
pub use locate::{locate, locate_all, script_dir, scan_dirs, user_font_dirs, FontFile, Located};
pub use used::{used_fonts, FontUse};

/// Errors returned by this crate.
#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    /// The file is not a parseable font.
    BadFont(String),
    /// `collect` was asked to write but neither a destination folder nor a zip path was given.
    NoOutput,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::BadFont(s) => write!(f, "not a valid font: {s}"),
            Error::NoOutput => write!(f, "no destination folder or zip path given"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}

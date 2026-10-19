//! Additional subtitle formats and ASS attachment helpers.
use crate::ass::{Event, Script, Style};

pub mod attach;
pub mod detect;
pub mod lrc;
pub mod markup;
pub mod microdvd;
pub mod sami;
pub mod sbv;
pub mod subviewer;
pub mod ttml;
pub mod txt;

/// Fresh script for an imported file (the default style is sized for the default PlayRes).
pub(crate) fn new_script() -> Script {
    let mut s = Script::new();
    s.info_set("Title", "Imported");
    s
}

pub(crate) fn sorted_events(script: &Script) -> Vec<&Event> {
    let mut evs: Vec<&Event> = script.events.iter().filter(|e| !e.is_comment()).collect();
    evs.sort_by_key(|e| (e.start, e.end));
    evs
}

/// Visible text lines of an event (tags stripped, `\N` splits lines, blank lines dropped).
pub(crate) fn plain_lines(e: &Event) -> Vec<String> {
    crate::tags::lines(&e.text).into_iter().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
}

/// Add a style named `name` (a copy of the default one) when missing.
pub(crate) fn ensure_style(script: &mut Script, name: &str) {
    if script.style(name).is_none() {
        let base: Style = script.styles.first().cloned().unwrap_or_default();
        script.styles.push(Style { name: name.to_string(), ..base });
    }
}

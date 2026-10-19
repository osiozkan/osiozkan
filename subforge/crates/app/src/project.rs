//! The open document: script + undo/redo + file state.
use std::path::PathBuf;
use std::time::{Duration, Instant};
use subforge_core::ass::{Event, EventKind, Script, Style};
use subforge_core::time::Ms;
use subforge_core::{self as core, timing, Format};

#[derive(Clone)]
struct Snapshot {
    label: String,
    script: Script,
}

pub struct Project {
    pub script: Script,
    pub path: Option<PathBuf>,
    pub dirty: bool,
    pub encoding: String,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<(String, Instant)>,
}

const MAX_UNDO: usize = 200;

impl Default for Project {
    fn default() -> Self {
        Project::new()
    }
}

impl Project {
    pub fn new() -> Project {
        let mut s = Script::new();
        s.events.push(Event::new(0, 3000, "Default", ""));
        Project { script: s, path: None, dirty: false, encoding: "UTF-8".into(), undo: vec![], redo: vec![], last_edit: None }
    }

    pub fn open(path: &str) -> Result<Project, String> {
        let (script, det) = core::load_path(path)?;
        Ok(Project { script, path: Some(PathBuf::from(path)), dirty: false, encoding: det.name.to_string(), undo: vec![], redo: vec![], last_edit: None })
    }

    pub fn from_script(script: Script) -> Project {
        Project { script, path: None, dirty: false, encoding: "UTF-8".into(), undo: vec![], redo: vec![], last_edit: None }
    }

    pub fn save(&mut self) -> Result<(), String> {
        let p = self.path.clone().ok_or("no path")?;
        self.save_as(p.to_string_lossy().as_ref())
    }

    pub fn save_as(&mut self, path: &str) -> Result<(), String> {
        core::save_path(&self.script, path)?;
        if Format::from_path(path) == Some(Format::Ass) || self.path.is_none() {
            self.path = Some(PathBuf::from(path));
            self.dirty = false;
        }
        Ok(())
    }

    pub fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
        format!("{}{}", if self.dirty { "*" } else { "" }, name)
    }

    // ------------------------------------------------------------ undo
    fn push_undo(&mut self, label: &str) {
        self.undo.push(Snapshot { label: label.to_string(), script: self.script.clone() });
        // snapshots are full copies: keep memory bounded on huge (FX) scripts
        let depth = (1_500_000 / self.script.events.len().max(1)).clamp(3, MAX_UNDO);
        while self.undo.len() > depth {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
    }

    /// Run an edit as one undo step.
    pub fn edit<R>(&mut self, label: &str, f: impl FnOnce(&mut Script) -> R) -> R {
        self.push_undo(label);
        self.last_edit = None;
        f(&mut self.script)
    }

    /// Like `edit` but successive edits with the same key within 1.2 s share one undo step
    /// (typing in the text box, dragging a value).
    pub fn edit_coalesced<R>(&mut self, key: &str, f: impl FnOnce(&mut Script) -> R) -> R {
        let now = Instant::now();
        let merge = matches!(&self.last_edit, Some((k, t)) if k == key && now.duration_since(*t) < Duration::from_millis(1200));
        if !merge {
            self.push_undo(key);
        } else {
            self.dirty = true;
        }
        self.last_edit = Some((key.to_string(), now));
        f(&mut self.script)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|s| s.label.as_str())
    }
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|s| s.label.as_str())
    }

    pub fn undo(&mut self) -> bool {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snapshot { label: s.label.clone(), script: std::mem::replace(&mut self.script, s.script) });
            self.dirty = true;
            self.last_edit = None;
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self) -> bool {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snapshot { label: s.label.clone(), script: std::mem::replace(&mut self.script, s.script) });
            self.dirty = true;
            self.last_edit = None;
            true
        } else {
            false
        }
    }

    // ------------------------------------------------------------ line ops
    pub fn insert_after(&mut self, idx: Option<usize>) -> usize {
        let n = self.script.events.len();
        let at = idx.map(|i| i + 1).unwrap_or(n).min(n);
        let (start, style) = match idx.and_then(|i| self.script.events.get(i)) {
            Some(e) => (e.end, e.style.clone()),
            None => (0, self.script.styles.first().map(|s| s.name.clone()).unwrap_or_else(|| "Default".into())),
        };
        self.edit("Insert line", |s| s.events.insert(at, Event::new(start, start + 2000, &style, "")));
        at
    }

    pub fn insert_before(&mut self, idx: usize) -> usize {
        let (end, style) = {
            let e = &self.script.events[idx.min(self.script.events.len().saturating_sub(1))];
            (e.start, e.style.clone())
        };
        self.edit("Insert line", |s| s.events.insert(idx, Event::new((end - 2000).max(0), end, &style, "")));
        idx
    }

    pub fn delete(&mut self, idx: &[usize]) {
        let mut v: Vec<usize> = idx.to_vec();
        v.sort_unstable();
        v.dedup();
        self.edit("Delete lines", |s| {
            for &i in v.iter().rev() {
                if i < s.events.len() {
                    s.events.remove(i);
                }
            }
        });
    }

    pub fn duplicate(&mut self, idx: &[usize]) -> Vec<usize> {
        let mut v: Vec<usize> = idx.to_vec();
        v.sort_unstable();
        v.dedup();
        let mut out = vec![];
        self.edit("Duplicate lines", |s| {
            let mut offset = 0;
            for &i in &v {
                let e = s.events[i + offset].clone();
                s.events.insert(i + offset + 1, e);
                out.push(i + offset + 1);
                offset += 1;
            }
        });
        out
    }

    pub fn toggle_comment(&mut self, idx: &[usize]) {
        self.edit("Toggle comment", |s| {
            for &i in idx {
                if let Some(e) = s.events.get_mut(i) {
                    e.kind = if e.kind == EventKind::Comment { EventKind::Dialogue } else { EventKind::Comment };
                }
            }
        });
    }

    /// Join selected (contiguous or not) lines into the first one.
    pub fn join(&mut self, idx: &[usize], sep: &str) -> Option<usize> {
        let mut v: Vec<usize> = idx.to_vec();
        v.sort_unstable();
        v.dedup();
        if v.len() < 2 {
            return None;
        }
        self.edit("Join lines", |s| {
            let group: Vec<Event> = v.iter().filter_map(|&i| s.events.get(i).cloned()).collect();
            let merged = timing::merge(&group, sep)?;
            s.events[v[0]] = merged;
            for &i in v[1..].iter().rev() {
                s.events.remove(i);
            }
            Some(v[0])
        })
    }

    pub fn split_at_char(&mut self, idx: usize, pos: usize) -> bool {
        self.edit("Split line", |s| {
            if let Some(e) = s.events.get_mut(idx) {
                if let Some(second) = timing::split_at_char(e, pos) {
                    s.events.insert(idx + 1, second);
                    return true;
                }
            }
            false
        })
    }

    pub fn shift(&mut self, idx: &[usize], delta: Ms, start: bool, end: bool) {
        self.edit("Shift times", |s| timing::shift(&mut s.events, idx, delta, start, end));
    }

    pub fn sort(&mut self, key: timing::SortKey) {
        self.edit("Sort lines", |s| timing::sort(&mut s.events, key));
    }

    // ------------------------------------------------------------ styles
    pub fn rename_style(&mut self, old: &str, new: &str) {
        self.edit("Rename style", |s| {
            for st in s.styles.iter_mut().filter(|st| st.name == old) {
                st.name = new.to_string();
            }
            for e in s.events.iter_mut().filter(|e| e.style == old) {
                e.style = new.to_string();
            }
        });
    }

    pub fn add_style(&mut self, st: Style) {
        self.edit("Add style", |s| s.styles.push(st));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_redo_and_ops() {
        let mut p = Project::new();
        p.script.events.clear();
        p.insert_after(None);
        p.insert_after(Some(0));
        assert_eq!(p.script.events.len(), 2);
        assert!(p.undo());
        assert_eq!(p.script.events.len(), 1);
        assert!(p.redo());
        assert_eq!(p.script.events.len(), 2);
        p.script.events[0].text = "a".into();
        p.script.events[1].text = "b".into();
        let j = p.join(&[0, 1], "\\N").unwrap();
        assert_eq!(p.script.events.len(), 1);
        assert_eq!(p.script.events[j].text, "a\\Nb");
        p.undo();
        assert_eq!(p.script.events.len(), 2);
        p.delete(&[0]);
        assert_eq!(p.script.events.len(), 1);
    }

    #[test]
    fn coalescing() {
        let mut p = Project::new();
        p.edit_coalesced("text:0", |s| s.events[0].text = "a".into());
        p.edit_coalesced("text:0", |s| s.events[0].text = "ab".into());
        p.edit_coalesced("text:0", |s| s.events[0].text = "abc".into());
        assert!(p.undo());
        assert_eq!(p.script.events[0].text, "");
        assert!(!p.can_undo());
    }

    #[test]
    fn duplicate_and_split() {
        let mut p = Project::new();
        p.script.events[0].text = "abcdef".into();
        p.script.events[0].end = 1000;
        let d = p.duplicate(&[0]);
        assert_eq!(d, vec![1]);
        assert!(p.split_at_char(0, 3));
        assert_eq!(p.script.events.len(), 3);
        assert_eq!(p.script.events[0].text, "abc");
    }
}

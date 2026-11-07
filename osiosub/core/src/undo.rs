//! Snapshot-based undo/redo (cheap enough for subtitle-sized documents).
use crate::ass::AssFile;

pub struct History {
    undo: Vec<(String, AssFile)>,
    redo: Vec<(String, AssFile)>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self { Self { undo: vec![], redo: vec![], limit } }
    /// Call BEFORE mutating `cur`.
    pub fn record(&mut self, label: &str, cur: &AssFile) {
        self.undo.push((label.to_string(), cur.clone()));
        if self.undo.len() > self.limit { self.undo.remove(0); }
        self.redo.clear();
    }
    pub fn undo(&mut self, cur: &mut AssFile) -> Option<String> {
        let (l, prev) = self.undo.pop()?;
        self.redo.push((l.clone(), std::mem::replace(cur, prev)));
        Some(l)
    }
    pub fn redo(&mut self, cur: &mut AssFile) -> Option<String> {
        let (l, next) = self.redo.pop()?;
        self.undo.push((l.clone(), std::mem::replace(cur, next)));
        Some(l)
    }
    pub fn can_undo(&self) -> bool { !self.undo.is_empty() }
    pub fn can_redo(&self) -> bool { !self.redo.is_empty() }
    pub fn undo_label(&self) -> Option<&str> { self.undo.last().map(|x| x.0.as_str()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn undo_redo() {
        let mut f = AssFile::new_default();
        let mut h = History::new(10);
        h.record("add", &f);
        f.events.push(Default::default());
        assert!(h.undo(&mut f).is_some());
        assert_eq!(f.events.len(), 0);
        assert!(h.redo(&mut f).is_some());
        assert_eq!(f.events.len(), 1);
    }
}

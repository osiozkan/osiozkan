//! Undo/redo as snapshots of the entry list (cheap enough for subtitle-sized files; bounded).

use crate::ass::Script;

pub struct History {
    undo: Vec<(String, Script)>,
    redo: Vec<(String, Script)>,
    limit: usize,
    /// Approximate memory budget for the undo stack, in bytes.
    budget: usize,
}

fn approx_size(s: &Script) -> usize {
    s.entries
        .iter()
        .map(|e| match e {
            crate::ass::Entry::Line(l) => 120 + l.text.len() + l.style.len() + l.actor.len() + l.effect.len(),
            crate::ass::Entry::Raw { text, .. } => 48 + text.len(),
            _ => 160,
        })
        .sum()
}

impl History {
    pub fn new(limit: usize) -> Self {
        History { undo: vec![], redo: vec![], limit, budget: 256 << 20 }
    }
    /// Call *before* mutating `current`.
    pub fn record(&mut self, label: &str, current: &Script) {
        self.undo.push((label.to_string(), current.clone()));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        let per = approx_size(current).max(1);
        while self.undo.len() > 1 && self.undo.len() * per > self.budget {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn undo(&mut self, current: &mut Script) -> Option<String> {
        let (label, prev) = self.undo.pop()?;
        self.redo.push((label.clone(), std::mem::replace(current, prev)));
        Some(label)
    }
    pub fn redo(&mut self, current: &mut Script) -> Option<String> {
        let (label, next) = self.redo.pop()?;
        self.undo.push((label.clone(), std::mem::replace(current, next)));
        Some(label)
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ass::Line;
    #[test]
    fn undo_redo() {
        let mut s = Script::new_default();
        let mut h = History::new(10);
        h.record("add", &s);
        s.push_line(Line::new(0, 1, "x"));
        assert_eq!(s.line_count(), 1);
        h.undo(&mut s);
        assert_eq!(s.line_count(), 0);
        h.redo(&mut s);
        assert_eq!(s.line_count(), 1);
        assert!(!h.can_redo());
    }
}

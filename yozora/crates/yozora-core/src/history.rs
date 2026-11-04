//! Geri al / yinele: belge anlık görüntüleri.
use crate::ass::Document;

pub struct History {
    undo: Vec<(String, Document)>,
    redo: Vec<(String, Document)>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        History { undo: Vec::new(), redo: Vec::new(), limit: limit.max(1) }
    }

    /// Değişiklikten ÖNCE çağır.
    pub fn record(&mut self, label: &str, before: &Document) {
        self.undo.push((label.to_string(), before.clone()));
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn undo(&mut self, current: &mut Document) -> Option<String> {
        let (label, prev) = self.undo.pop()?;
        let cur = std::mem::replace(current, prev);
        self.redo.push((label.clone(), cur));
        Some(label)
    }

    pub fn redo(&mut self, current: &mut Document) -> Option<String> {
        let (label, next) = self.redo.pop()?;
        let cur = std::mem::replace(current, next);
        self.undo.push((label.clone(), cur));
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
    use crate::ass::Event;
    #[test]
    fn undo_redo() {
        let mut d = Document::new_default();
        let mut h = History::new(10);
        h.record("ekle", &d);
        d.events.push(Event::default());
        assert!(h.undo(&mut d).is_some());
        assert!(d.events.is_empty());
        assert!(h.redo(&mut d).is_some());
        assert_eq!(d.events.len(), 1);
        assert!(!h.can_redo());
    }
}

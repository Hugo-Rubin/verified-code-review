//! The editor: runs operations and decides when the document needs saving.

use crate::doc::{Doc, Op};

pub struct Editor {
    doc: Doc,
    dirty: bool,
    saves: usize,
}

impl Editor {
    pub fn new(doc: Doc) -> Self {
        Self {
            doc,
            dirty: false,
            saves: 0,
        }
    }

    pub fn doc(&self) -> &Doc {
        &self.doc
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// How many times this editor has written the document out.
    pub fn saves(&self) -> usize {
        self.saves
    }

    /// Run one operation against the document.
    pub fn run(&mut self, op: Op) {
        self.doc.apply(&op);
        self.dirty = true;
    }

    /// Write the document out if it has changed since the last save.
    pub fn save_if_needed(&mut self) -> bool {
        if !self.dirty {
            return false;
        }
        self.saves += 1;
        self.dirty = false;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::new(Doc::new(vec!["one".into(), "two".into()]))
    }

    #[test]
    fn an_operation_reaches_the_document() {
        let mut e = editor();
        e.run(Op::Insert {
            at: 1,
            text: "mid".into(),
        });
        assert_eq!(e.doc().lines(), vec!["one", "mid", "two"]);
    }

    #[test]
    fn a_fresh_editor_has_nothing_to_save() {
        let mut e = editor();
        assert!(!e.save_if_needed());
        assert_eq!(e.saves(), 0);
    }
}

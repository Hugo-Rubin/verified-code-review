//! Documents and the operations that edit them.

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Insert { at: usize, text: String },
    Remove { at: usize },
}

/// A cheap handle onto a shared line buffer.
///
/// `Doc` is a handle, not a value. Cloning one yields a second handle onto the
/// *same* text, which is what lets panes, views and undo stacks pass documents
/// around without copying them.
#[derive(Debug, Clone, Default)]
pub struct Doc {
    lines: Rc<RefCell<Vec<String>>>,
}

impl Doc {
    pub fn new(lines: Vec<String>) -> Self {
        Self {
            lines: Rc::new(RefCell::new(lines)),
        }
    }

    pub fn len(&self) -> usize {
        self.lines.borrow().len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.borrow().is_empty()
    }

    /// A copy of the current text.
    pub fn lines(&self) -> Vec<String> {
        self.lines.borrow().clone()
    }

    /// Order-sensitive checksum of the current text.
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for line in self.lines.borrow().iter() {
            for b in line.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
            h ^= 0xff;
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }

    /// Apply `op`. Positions outside the document are ignored.
    pub fn apply(&self, op: &Op) {
        let mut lines = self.lines.borrow_mut();
        match op {
            Op::Insert { at, text } => {
                if *at <= lines.len() {
                    lines.insert(*at, text.clone());
                }
            }
            Op::Remove { at } => {
                if *at < lines.len() {
                    lines.remove(*at);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Doc {
        Doc::new(vec!["one".into(), "two".into()])
    }

    #[test]
    fn inserting_adds_a_line() {
        let d = sample();
        d.apply(&Op::Insert {
            at: 0,
            text: "zero".into(),
        });
        assert_eq!(d.lines(), vec!["zero", "one", "two"]);
    }

    #[test]
    fn removing_drops_a_line() {
        let d = sample();
        d.apply(&Op::Remove { at: 1 });
        assert_eq!(d.lines(), vec!["one"]);
    }

    #[test]
    fn a_position_outside_the_document_is_ignored() {
        let d = sample();
        d.apply(&Op::Remove { at: 99 });
        assert_eq!(d.len(), 2);
        assert!(!d.is_empty());
    }

    #[test]
    fn the_checksum_follows_the_text() {
        let d = sample();
        let before = d.checksum();
        d.apply(&Op::Insert {
            at: 0,
            text: "zero".into(),
        });
        assert_ne!(before, d.checksum());
    }
}

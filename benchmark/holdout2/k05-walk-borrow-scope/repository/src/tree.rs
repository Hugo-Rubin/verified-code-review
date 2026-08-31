//! The shared tree the walker traverses.

use std::cell::RefCell;
use std::rc::Rc;

/// A node in a shared tree. Nodes are shared, so they are handed around as
/// `Rc<RefCell<Node>>`.
pub struct Node {
    pub name: String,
    pub depth: u32,
    pub children: Vec<Rc<RefCell<Node>>>,
}

impl Node {
    pub fn leaf(name: &str) -> Rc<RefCell<Node>> {
        Node::branch(name, Vec::new())
    }

    pub fn branch(name: &str, children: Vec<Rc<RefCell<Node>>>) -> Rc<RefCell<Node>> {
        Rc::new(RefCell::new(Node {
            name: name.to_string(),
            depth: 0,
            children,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_branch_owns_its_children() {
        let tree = Node::branch("root", vec![Node::leaf("a"), Node::leaf("b")]);
        assert_eq!(tree.borrow().children.len(), 2);
        assert_eq!(tree.borrow().children[0].borrow().name, "a");
        assert_eq!(tree.borrow().depth, 0);
    }
}

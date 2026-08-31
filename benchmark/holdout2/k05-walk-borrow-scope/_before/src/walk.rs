//! Depth-first traversal of a shared tree.

use crate::tree::Node;
use std::cell::RefCell;
use std::rc::Rc;

/// Called once per node, in depth-first pre-order.
pub trait Visitor {
    fn visit(&mut self, node: &Rc<RefCell<Node>>, depth: u32);
}

/// Visit `node` and everything beneath it.
pub fn walk(node: &Rc<RefCell<Node>>, visitor: &mut dyn Visitor, depth: u32) {
    let children = node.borrow().children.clone();
    visitor.visit(node, depth);
    for child in children.iter() {
        walk(child, visitor, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::visitors::NameCollector;

    #[test]
    fn walk_visits_pre_order() {
        let tree = Node::branch(
            "root",
            vec![Node::branch("a", vec![Node::leaf("a1")]), Node::leaf("b")],
        );
        let mut collector = NameCollector::default();
        walk(&tree, &mut collector, 0);
        assert_eq!(
            collector.names,
            vec![
                "root".to_string(),
                "a".to_string(),
                "a1".to_string(),
                "b".to_string()
            ]
        );
    }

    #[test]
    fn walk_of_a_leaf_visits_once() {
        let tree = Node::leaf("only");
        let mut collector = NameCollector::default();
        walk(&tree, &mut collector, 0);
        assert_eq!(collector.names, vec!["only".to_string()]);
    }
}

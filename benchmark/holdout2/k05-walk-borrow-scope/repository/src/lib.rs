//! A tiny tree walker with a couple of ready-made visitors.

pub mod tree;
pub mod visitors;
pub mod walk;

use crate::tree::Node;
use crate::visitors::{DepthMarker, NameCollector};
use crate::walk::walk;
use std::cell::RefCell;
use std::rc::Rc;

/// Every node name under `root`, in depth-first pre-order.
pub fn node_names(root: &Rc<RefCell<Node>>) -> Vec<String> {
    let mut collector = NameCollector::default();
    walk(root, &mut collector, 0);
    collector.names
}

/// Record each node's distance from `root` in the node itself, returning how
/// many nodes were marked.
pub fn mark_depths(root: &Rc<RefCell<Node>>) -> usize {
    let mut marker = DepthMarker::default();
    walk(root, &mut marker, 0);
    marker.marked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_names_lists_the_whole_tree() {
        let tree = Node::branch("root", vec![Node::leaf("a"), Node::leaf("b")]);
        assert_eq!(
            node_names(&tree),
            vec!["root".to_string(), "a".to_string(), "b".to_string()]
        );
    }
}

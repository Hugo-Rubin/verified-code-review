//! The visitors shipped with the crate.

use crate::tree::Node;
use crate::walk::Visitor;
use std::cell::RefCell;
use std::rc::Rc;

/// Collects every node name in traversal order.
#[derive(Default)]
pub struct NameCollector {
    pub names: Vec<String>,
}

impl Visitor for NameCollector {
    fn visit(&mut self, node: &Rc<RefCell<Node>>, _depth: u32) {
        self.names.push(node.borrow().name.clone());
    }
}

/// Writes each node's distance from the root back into the node.
#[derive(Default)]
pub struct DepthMarker {
    pub marked: usize,
}

impl Visitor for DepthMarker {
    fn visit(&mut self, node: &Rc<RefCell<Node>>, depth: u32) {
        node.borrow_mut().depth = depth;
        self.marked += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_collector_records_the_node_name() {
        let node = Node::leaf("a");
        let mut collector = NameCollector::default();
        collector.visit(&node, 0);
        assert_eq!(collector.names, vec!["a".to_string()]);
    }

    #[test]
    fn the_depth_marker_writes_the_depth_into_the_node() {
        let node = Node::leaf("a");
        let mut marker = DepthMarker::default();
        marker.visit(&node, 3);
        assert_eq!(node.borrow().depth, 3);
        assert_eq!(marker.marked, 1);
    }
}

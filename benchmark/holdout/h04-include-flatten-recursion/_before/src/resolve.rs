//! Flattening an include graph into a load order.

use crate::graph::IncludeGraph;
use std::collections::BTreeSet;

/// Every unit reachable from `root`, `root` first, in depth-first order.
pub fn flatten(graph: &IncludeGraph, root: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        for child in graph.includes_of(&name).iter().rev() {
            stack.push(child.clone());
        }
        out.push(name);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(spec: &[(&str, &[&str])]) -> IncludeGraph {
        let pairs = spec
            .iter()
            .map(|(n, inc)| {
                (
                    n.to_string(),
                    inc.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )
            })
            .collect();
        IncludeGraph::from_pairs(pairs).expect("test graph should be valid")
    }

    #[test]
    fn a_leaf_flattens_to_itself() {
        let g = graph(&[("base", &[])]);
        assert_eq!(flatten(&g, "base"), vec!["base".to_string()]);
    }

    #[test]
    fn nested_includes_come_out_depth_first() {
        let g = graph(&[
            ("app", &["db", "http"]),
            ("db", &["base"]),
            ("http", &[]),
            ("base", &[]),
        ]);
        assert_eq!(
            flatten(&g, "app"),
            vec![
                "app".to_string(),
                "db".to_string(),
                "base".to_string(),
                "http".to_string(),
            ]
        );
    }

    #[test]
    fn an_undeclared_root_flattens_to_itself() {
        let g = graph(&[("base", &[])]);
        assert_eq!(flatten(&g, "nope"), vec!["nope".to_string()]);
    }
}

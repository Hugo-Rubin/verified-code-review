//! The validated include graph.

use std::collections::BTreeMap;

/// The largest number of config units one graph may describe.
pub const MAX_UNITS: usize = 64;

#[derive(Debug, PartialEq, Eq)]
pub enum GraphError {
    TooManyUnits,
    UnknownInclude(String),
    IncludedTwice(String),
    Cycle(String),
}

/// A validated include graph.
///
/// `edges` is private and no method mutates it, so every value of this type
/// satisfies the four conditions `from_pairs` checks.
#[derive(Debug)]
pub struct IncludeGraph {
    edges: BTreeMap<String, Vec<String>>,
}

impl IncludeGraph {
    /// Build a graph from `(unit, includes)` pairs, rejecting any input that
    /// is too large, names an undeclared unit, includes a unit from more than
    /// one place, or contains a cycle.
    pub fn from_pairs(pairs: Vec<(String, Vec<String>)>) -> Result<Self, GraphError> {
        if pairs.len() > MAX_UNITS {
            return Err(GraphError::TooManyUnits);
        }
        let edges: BTreeMap<String, Vec<String>> = pairs.into_iter().collect();

        let mut includers: BTreeMap<&str, usize> = BTreeMap::new();
        for includes in edges.values() {
            for target in includes {
                if !edges.contains_key(target) {
                    return Err(GraphError::UnknownInclude(target.clone()));
                }
                let n = includers.entry(target.as_str()).or_insert(0);
                *n += 1;
                if *n > 1 {
                    return Err(GraphError::IncludedTwice(target.clone()));
                }
            }
        }

        let graph = IncludeGraph { edges };
        let mut state: BTreeMap<&str, u8> = BTreeMap::new();
        for name in graph.edges.keys() {
            graph.mark(name, &mut state)?;
        }
        Ok(graph)
    }

    /// Depth-first cycle detection. 1 = on the current path, 2 = cleared.
    fn mark<'a>(&'a self, name: &'a str, state: &mut BTreeMap<&'a str, u8>) -> Result<(), GraphError> {
        match state.get(name) {
            Some(2) => return Ok(()),
            Some(_) => return Err(GraphError::Cycle(name.to_string())),
            None => {}
        }
        state.insert(name, 1);
        for child in self.includes_of(name) {
            self.mark(child.as_str(), state)?;
        }
        state.insert(name, 2);
        Ok(())
    }

    /// The units `name` pulls in directly. Empty for a name the graph does
    /// not declare.
    pub fn includes_of(&self, name: &str) -> &[String] {
        self.edges
            .get(name)
            .map(|v| v.as_slice())
            .unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(spec: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        spec.iter()
            .map(|(n, inc)| {
                (
                    n.to_string(),
                    inc.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                )
            })
            .collect()
    }

    #[test]
    fn accepts_a_chain() {
        let g = IncludeGraph::from_pairs(pairs(&[("a", &["b"]), ("b", &["c"]), ("c", &[])])).unwrap();
        assert_eq!(g.len(), 3);
        assert_eq!(g.includes_of("a"), ["b".to_string()]);
    }

    #[test]
    fn rejects_an_undeclared_include() {
        let e = IncludeGraph::from_pairs(pairs(&[("a", &["ghost"])])).unwrap_err();
        assert_eq!(e, GraphError::UnknownInclude("ghost".to_string()));
    }

    #[test]
    fn rejects_a_unit_included_from_two_places() {
        let e =
            IncludeGraph::from_pairs(pairs(&[("a", &["c"]), ("b", &["c"]), ("c", &[])])).unwrap_err();
        assert_eq!(e, GraphError::IncludedTwice("c".to_string()));
    }

    #[test]
    fn rejects_a_cycle() {
        let e = IncludeGraph::from_pairs(pairs(&[("a", &["b"]), ("b", &["a"])])).unwrap_err();
        assert!(matches!(e, GraphError::Cycle(_)));
    }

    #[test]
    fn rejects_a_graph_larger_than_the_cap() {
        let spec: Vec<(String, Vec<String>)> =
            (0..=MAX_UNITS).map(|i| (format!("u{i}"), Vec::new())).collect();
        assert_eq!(
            IncludeGraph::from_pairs(spec).unwrap_err(),
            GraphError::TooManyUnits
        );
    }
}

//! Internal import graph: fan-in/out, cycles (Tarjan SCC) and closures.

use std::collections::VecDeque;

use petgraph::algo::tarjan_scc;
use petgraph::graph::DiGraph;

use crate::model::{ImportTarget, ModuleFacts};

/// A deduplicated edge between two modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// Target module index (source index for incoming edges).
    pub to: usize,
    /// True when every import between the two modules is `import type`.
    pub type_only: bool,
}

/// Import graph over module indices (same order as the module list).
///
/// Edges from test files are left out, so tests never inflate fan-in or blast radius.
#[derive(Debug, Clone)]
pub struct ImportGraph {
    pub out: Vec<Vec<Edge>>,
    pub inc: Vec<Vec<Edge>>,
    /// Cycle groups (SCCs with more than one module) over value edges, each sorted.
    pub cycles: Vec<Vec<usize>>,
    /// Index into `cycles` for modules that are in one.
    pub cycle_of: Vec<Option<usize>>,
}

impl ImportGraph {
    /// Builds the graph from resolved imports.
    pub fn build(modules: &[ModuleFacts]) -> ImportGraph {
        let index: std::collections::HashMap<&str, usize> =
            modules.iter().enumerate().map(|(i, m)| (m.path.as_str(), i)).collect();
        let n = modules.len();
        let mut out: Vec<Vec<Edge>> = vec![Vec::new(); n];

        for (i, m) in modules.iter().enumerate() {
            if m.is_test {
                continue;
            }
            for imp in &m.imports {
                let ImportTarget::Internal(p) = &imp.target else { continue };
                let Some(&j) = index.get(p.as_str()) else { continue };
                if j == i {
                    continue;
                }
                let type_only = imp.kind.is_type_only();
                match out[i].iter_mut().find(|e| e.to == j) {
                    Some(e) => e.type_only &= type_only,
                    None => out[i].push(Edge { to: j, type_only }),
                }
            }
            out[i].sort_by_key(|e| e.to);
        }

        let mut inc: Vec<Vec<Edge>> = vec![Vec::new(); n];
        for (i, edges) in out.iter().enumerate() {
            for e in edges {
                inc[e.to].push(Edge { to: i, type_only: e.type_only });
            }
        }

        let cycles = find_cycles(&out);
        let mut cycle_of = vec![None; n];
        for (c, group) in cycles.iter().enumerate() {
            for &m in group {
                cycle_of[m] = Some(c);
            }
        }
        ImportGraph { out, inc, cycles, cycle_of }
    }

    /// `(value, type_only)` counts of internal dependencies.
    pub fn fan_out(&self, i: usize) -> (usize, usize) {
        split(&self.out[i])
    }

    /// `(value, type_only)` counts of internal dependents.
    pub fn fan_in(&self, i: usize) -> (usize, usize) {
        split(&self.inc[i])
    }

    /// Forward reachability over value edges: `(module, distance)` pairs, sorted by module.
    pub fn dependencies(&self, i: usize) -> Vec<(usize, u32)> {
        bfs(&self.out, i, true)
    }

    /// Reverse reachability over all edges: `(module, distance)` pairs, sorted by module.
    pub fn dependents(&self, i: usize) -> Vec<(usize, u32)> {
        bfs(&self.inc, i, false)
    }
}

fn split(edges: &[Edge]) -> (usize, usize) {
    let t = edges.iter().filter(|e| e.type_only).count();
    (edges.len() - t, t)
}

fn bfs(adj: &[Vec<Edge>], start: usize, value_only: bool) -> Vec<(usize, u32)> {
    let mut dist = vec![u32::MAX; adj.len()];
    dist[start] = 0;
    let mut queue = VecDeque::from([start]);
    let mut found = Vec::new();
    while let Some(u) = queue.pop_front() {
        for e in &adj[u] {
            if (value_only && e.type_only) || dist[e.to] != u32::MAX {
                continue;
            }
            dist[e.to] = dist[u] + 1;
            found.push((e.to, dist[e.to]));
            queue.push_back(e.to);
        }
    }
    found.sort_unstable();
    found
}

fn find_cycles(out: &[Vec<Edge>]) -> Vec<Vec<usize>> {
    let mut g = DiGraph::<(), ()>::with_capacity(out.len(), 0);
    let nodes: Vec<_> = (0..out.len()).map(|_| g.add_node(())).collect();
    for (i, edges) in out.iter().enumerate() {
        for e in edges.iter().filter(|e| !e.type_only) {
            g.add_edge(nodes[i], nodes[e.to], ());
        }
    }
    let mut cycles: Vec<Vec<usize>> = tarjan_scc(&g)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            let mut v: Vec<usize> = scc.into_iter().map(|n| n.index()).collect();
            v.sort_unstable();
            v
        })
        .collect();
    cycles.sort();
    cycles
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Import, ImportKind};

    fn module(path: &str, deps: &[(&str, ImportKind)]) -> ModuleFacts {
        ModuleFacts {
            path: path.into(),
            is_test: false,
            loc: 10,
            imports: deps
                .iter()
                .map(|(d, k)| Import { specifier: d.to_string(), kind: *k, target: ImportTarget::Internal(d.to_string()), line: 1 })
                .collect(),
            exports: 0,
            top_level_decls: 0,
            functions: vec![],
            top_level_cognitive: 0,
            symbols: vec![],
            parse_errors: vec![],
        }
    }

    #[test]
    fn cycles_closures_and_type_edges() {
        use ImportKind::*;
        let mods = vec![
            module("a", &[("b", Static)]),
            module("b", &[("c", Static)]),
            module("c", &[("a", Static), ("d", TypeOnly)]),
            module("d", &[("e", Static)]),
            module("e", &[]),
        ];
        let g = ImportGraph::build(&mods);
        assert_eq!(g.cycles, vec![vec![0, 1, 2]]);
        assert_eq!(g.fan_out(2), (1, 1));
        assert_eq!(g.dependencies(0), vec![(1, 1), (2, 2)]);
        assert_eq!(g.dependents(4).len(), 4);
    }

    #[test]
    fn type_only_cycle_is_not_a_cycle() {
        use ImportKind::*;
        let g = ImportGraph::build(&[module("a", &[("b", TypeOnly)]), module("b", &[("a", Static)])]);
        assert!(g.cycles.is_empty());
    }
}

use std::collections::{HashMap, HashSet};

pub struct TarjanScc {
    graph: HashMap<String, HashSet<String>>,
    index_counter: usize,
    stack: Vec<String>,
    lowlinks: HashMap<String, usize>,
    index: HashMap<String, usize>,
    on_stack: HashSet<String>,
    sccs: Vec<Vec<String>>,
}

impl TarjanScc {
    pub fn new(graph: HashMap<String, HashSet<String>>) -> Self {
        Self {
            graph,
            index_counter: 0,
            stack: Vec::new(),
            lowlinks: HashMap::new(),
            index: HashMap::new(),
            on_stack: HashSet::new(),
            sccs: Vec::new(),
        }
    }

    pub fn find_cycles(mut self) -> Vec<Vec<String>> {
        let nodes: Vec<String> = self.graph.keys().cloned().collect();
        for node in nodes {
            if !self.index.contains_key(&node) {
                self.strongconnect(node);
            }
        }
        self.sccs.into_iter().filter(|scc| scc.len() > 1).collect()
    }

    fn strongconnect(&mut self, v: String) {
        let v_index = self.index_counter;
        self.index.insert(v.clone(), v_index);
        self.lowlinks.insert(v.clone(), v_index);
        self.index_counter += 1;
        self.stack.push(v.clone());
        self.on_stack.insert(v.clone());

        let neighbors: Vec<String> = self.graph
            .get(&v)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect();

        for w in neighbors {
            if !self.index.contains_key(&w) {
                self.strongconnect(w.clone());
                let w_low = *self.lowlinks.get(&w).unwrap_or(&usize::MAX);
                let v_low = *self.lowlinks.get(&v).unwrap_or(&usize::MAX);
                self.lowlinks.insert(v.clone(), v_low.min(w_low));
            } else if self.on_stack.contains(&w) {
                let w_idx = *self.index.get(&w).unwrap_or(&usize::MAX);
                let v_low = *self.lowlinks.get(&v).unwrap_or(&usize::MAX);
                self.lowlinks.insert(v.clone(), v_low.min(w_idx));
            }
        }

        if self.lowlinks.get(&v) == self.index.get(&v) {
            let mut scc = Vec::new();
            loop {
                let w = self.stack.pop().unwrap();
                self.on_stack.remove(&w);
                scc.push(w.clone());
                if w == v {
                    break;
                }
            }
            self.sccs.push(scc);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cycles() {
        let mut graph = HashMap::new();
        graph.insert("a".to_string(), {
            let mut s = HashSet::new();
            s.insert("b".to_string());
            s
        });
        graph.insert("b".to_string(), HashSet::new());
        let cycles = TarjanScc::new(graph).find_cycles();
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_simple_cycle() {
        let mut graph = HashMap::new();
        graph.insert("a".to_string(), {
            let mut s = HashSet::new();
            s.insert("b".to_string());
            s
        });
        graph.insert("b".to_string(), {
            let mut s = HashSet::new();
            s.insert("a".to_string());
            s
        });
        let cycles = TarjanScc::new(graph).find_cycles();
        assert_eq!(cycles.len(), 1);
    }
}

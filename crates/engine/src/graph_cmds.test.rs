#[cfg(test)]
mod delete_tests {
    use crate::{execute, sample_session};
    use serde_json::json;

    /// Deleting a graph that sits before the open one must not switch the user to another graph.
    #[test]
    fn deleting_an_earlier_graph_keeps_the_open_graph_open() {
        let mut s = sample_session();
        let run = |s: &mut crate::Session, id: &str, p: serde_json::Value| execute(s, id, &p).unwrap_or_else(|e| panic!("{id}: {e}"));
        run(&mut s, "file.new", json!({"template": "empty", "name": "Mine"}));
        for _ in 0..3 {
            run(&mut s, "graph.duplicate", json!({}));
        }
        let count = s.pkg.graphs.len();
        assert!(count >= 4, "need four graphs, got {count}");
        run(&mut s, "graph.open", json!({"graph": 2}));
        let open_name = s.pkg.graphs[s.graph].name.clone();
        run(&mut s, "graph.delete", json!({"graph": 0}));
        assert_eq!(s.pkg.graphs.len(), count - 1);
        assert_eq!(s.pkg.graphs[s.graph].name, open_name, "the open graph changed");
    }
}

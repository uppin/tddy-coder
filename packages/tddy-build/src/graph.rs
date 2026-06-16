//! Build DAG: assembly, topological sort, cycle detection, execution waves.

use std::collections::HashMap;

use crate::builtin::{self, GROUP};
use crate::error::BuildError;
use crate::lower::lower_target;
use crate::manifest::{BuildManifest, BuildTarget};
use crate::plugin::PluginRegistry;
use crate::proto::BuildAction;

/// Global build graph assembled from all discovered manifests.
///
/// Action nodes are identified globally as `"{target_id}::{action_id}"`. Edges
/// come from target-level `deps`/group membership and action-level input/output
/// overlap.
#[derive(Debug)]
pub struct BuildGraph {
    targets: HashMap<String, BuildTarget>,
    /// Insertion order of target ids (stable iteration for waves/output).
    order: Vec<String>,
}

impl BuildGraph {
    /// Flatten every target across `manifests` into a graph, rejecting duplicate
    /// ids and dependency cycles.
    pub fn from_manifests(manifests: Vec<BuildManifest>) -> Result<Self, BuildError> {
        let mut targets = HashMap::new();
        let mut order = Vec::new();
        for manifest in manifests {
            for target in manifest.targets {
                if targets.contains_key(&target.id) {
                    return Err(BuildError::Manifest(format!(
                        "duplicate target id: {}",
                        target.id
                    )));
                }
                order.push(target.id.clone());
                targets.insert(target.id.clone(), target);
            }
        }

        let graph = Self { targets, order };
        graph.check_target_cycles()?;
        Ok(graph)
    }

    /// Look up a target by id.
    pub fn target(&self, id: &str) -> Option<&BuildTarget> {
        self.targets.get(id)
    }

    /// All lowered actions for `target_id`, in declaration order.
    pub fn actions_for(
        &self,
        target_id: &str,
        registry: &PluginRegistry,
    ) -> Result<Vec<BuildAction>, BuildError> {
        let target = self
            .targets
            .get(target_id)
            .ok_or_else(|| BuildError::UnknownTarget(target_id.to_string()))?;
        lower_target(target, registry)
    }

    /// Targets to build for `target_id`, dependencies (and group members) first,
    /// ending with `target_id` itself.
    pub fn build_order(&self, target_id: &str) -> Result<Vec<String>, BuildError> {
        if !self.targets.contains_key(target_id) {
            return Err(BuildError::UnknownTarget(target_id.to_string()));
        }
        let mut visited = Vec::new();
        let mut on_stack = Vec::new();
        self.visit_build_order(target_id, &mut visited, &mut on_stack)?;
        Ok(visited)
    }

    fn visit_build_order(
        &self,
        id: &str,
        visited: &mut Vec<String>,
        on_stack: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        if visited.iter().any(|v| v == id) {
            return Ok(());
        }
        if on_stack.iter().any(|v| v == id) {
            return Err(BuildError::Cycle(format!("cycle through target {id}")));
        }
        on_stack.push(id.to_string());
        if let Some(target) = self.targets.get(id) {
            for dep in self.target_edges(target)? {
                self.visit_build_order(&dep, visited, on_stack)?;
            }
        }
        on_stack.pop();
        visited.push(id.to_string());
        Ok(())
    }

    /// Topologically-ordered execution waves of global action node ids; actions
    /// in the same wave may run in parallel.
    pub fn waves(&self, registry: &PluginRegistry) -> Result<Vec<Vec<String>>, BuildError> {
        // Collect all actions across all targets in stable order.
        let mut nodes: Vec<String> = Vec::new();
        let mut node_index: HashMap<String, usize> = HashMap::new();
        let mut actions: Vec<BuildAction> = Vec::new();
        let mut owner: Vec<String> = Vec::new();
        for target_id in &self.order {
            for action in self.actions_for(target_id, registry)? {
                let node = format!("{target_id}::{}", action.id);
                node_index.insert(node.clone(), nodes.len());
                nodes.push(node);
                owner.push(target_id.clone());
                actions.push(action);
            }
        }

        let mut edges = action_overlap_edges(&actions);

        // Cross-target dep edges: every action of a dep target precedes every
        // action of the dependent target.
        for (dependent_idx, dependent_target) in owner.iter().enumerate() {
            if let Some(target) = self.targets.get(dependent_target) {
                for dep in self.target_edges(target)? {
                    for (producer_idx, producer_target) in owner.iter().enumerate() {
                        if producer_target == &dep {
                            edges.push((producer_idx, dependent_idx));
                        }
                    }
                }
            }
        }

        let index_waves = kahn_waves(nodes.len(), &edges)?;
        Ok(index_waves
            .into_iter()
            .map(|wave| {
                let mut ids: Vec<String> = wave.into_iter().map(|i| nodes[i].clone()).collect();
                ids.sort();
                ids
            })
            .collect())
    }

    fn check_target_cycles(&self) -> Result<(), BuildError> {
        for id in &self.order {
            let mut visited = Vec::new();
            let mut on_stack = Vec::new();
            self.visit_build_order(id, &mut visited, &mut on_stack)?;
        }
        Ok(())
    }

    /// Target-level predecessors: declared deps plus group members.
    fn target_edges(&self, target: &BuildTarget) -> Result<Vec<String>, BuildError> {
        let mut edges = target.deps.clone();
        if let Some(config) = &target.config {
            if config.r#type == GROUP {
                edges.extend(builtin::group_member_ids(&config.fields)?);
            }
        }
        Ok(edges)
    }
}

/// Group `actions` into topological waves by input/output overlap. Returns the
/// indices into `actions`, grouped per wave.
pub fn action_waves(actions: &[BuildAction]) -> Result<Vec<Vec<usize>>, BuildError> {
    let edges = action_overlap_edges(actions);
    kahn_waves(actions.len(), &edges)
}

/// Edge `(producer, consumer)` whenever a consumer input glob matches a producer
/// output path.
fn action_overlap_edges(actions: &[BuildAction]) -> Vec<(usize, usize)> {
    let mut edges = Vec::new();
    for (producer_idx, producer) in actions.iter().enumerate() {
        for output in &producer.outputs {
            for (consumer_idx, consumer) in actions.iter().enumerate() {
                if producer_idx == consumer_idx {
                    continue;
                }
                if consumer_consumes(consumer, &output.path) {
                    edges.push((producer_idx, consumer_idx));
                }
            }
        }
    }
    edges
}

fn consumer_consumes(consumer: &BuildAction, output_path: &str) -> bool {
    for file_set in &consumer.inputs {
        for include in &file_set.include {
            if glob_matches(&file_set.root, include, output_path) {
                return true;
            }
        }
    }
    false
}

fn glob_matches(root: &str, include: &str, candidate: &str) -> bool {
    let mut patterns = vec![include.to_string()];
    if !root.is_empty() && root != "." {
        patterns.push(format!("{}/{}", root.trim_end_matches('/'), include));
    }
    patterns.iter().any(|p| match glob::Pattern::new(p) {
        Ok(pattern) => pattern.matches(candidate),
        Err(_) => p == candidate,
    })
}

/// Kahn's algorithm producing topological waves. Errors on cycles.
fn kahn_waves(n: usize, edges: &[(usize, usize)]) -> Result<Vec<Vec<usize>>, BuildError> {
    let mut indegree = vec![0usize; n];
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(from, to) in edges {
        adjacency[from].push(to);
        indegree[to] += 1;
    }

    let mut current: Vec<usize> = (0..n).filter(|&i| indegree[i] == 0).collect();
    current.sort_unstable();

    let mut waves = Vec::new();
    let mut consumed = 0;
    while !current.is_empty() {
        consumed += current.len();
        let mut next = Vec::new();
        for &node in &current {
            for &neighbor in &adjacency[node] {
                indegree[neighbor] -= 1;
                if indegree[neighbor] == 0 {
                    next.push(neighbor);
                }
            }
        }
        waves.push(current);
        next.sort_unstable();
        current = next;
    }

    if consumed != n {
        return Err(BuildError::Cycle(
            "action dependency cycle detected".to_string(),
        ));
    }
    Ok(waves)
}

#[cfg(test)]
mod tests {
    use super::{action_waves, BuildGraph};
    use crate::error::BuildError;
    use crate::manifest::{BuildManifest, BuildTarget, TargetConfig};
    use crate::plugin::PluginRegistry;
    use crate::proto::{ActionType, BuildAction, FileSet, OutputDecl, OutputKind};

    fn config(type_name: &str, fields_yaml: &str) -> TargetConfig {
        TargetConfig {
            r#type: type_name.to_string(),
            fields: serde_yaml::from_str(fields_yaml).expect("valid config fields"),
        }
    }

    fn script_target(id: &str, deps: Vec<&str>) -> BuildTarget {
        BuildTarget {
            id: id.to_string(),
            deps: deps.into_iter().map(String::from).collect(),
            config: Some(config("script", "command: [\"true\"]")),
            ..Default::default()
        }
    }

    fn manifest(targets: Vec<BuildTarget>) -> BuildManifest {
        BuildManifest {
            schema_version: 1,
            targets,
        }
    }

    fn action(id: &str, inputs: &[&str], outputs: &[&str]) -> BuildAction {
        BuildAction {
            id: id.to_string(),
            r#type: ActionType::Command as i32,
            command: vec!["true".to_string()],
            inputs: inputs
                .iter()
                .map(|p| FileSet {
                    include: vec![p.to_string()],
                    root: ".".to_string(),
                    ..Default::default()
                })
                .collect(),
            outputs: outputs
                .iter()
                .map(|p| OutputDecl {
                    path: p.to_string(),
                    kind: OutputKind::File as i32,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn duplicate_target_ids_are_rejected() {
        let err = BuildGraph::from_manifests(vec![manifest(vec![
            script_target("dup", vec![]),
            script_target("dup", vec![]),
        ])])
        .expect_err("duplicate ids must error");
        assert!(matches!(err, BuildError::Manifest(_)));
    }

    #[test]
    fn dependency_cycle_is_detected() {
        let err = BuildGraph::from_manifests(vec![manifest(vec![
            script_target("a", vec!["b"]),
            script_target("b", vec!["a"]),
        ])])
        .expect_err("a<->b cycle must error");
        assert!(matches!(err, BuildError::Cycle(_)));
    }

    #[test]
    fn build_order_lists_dependencies_before_dependents() {
        let graph = BuildGraph::from_manifests(vec![manifest(vec![
            script_target("app", vec!["core"]),
            script_target("core", vec![]),
        ])])
        .unwrap();
        let order = graph.build_order("app").unwrap();
        let core = order.iter().position(|t| t == "core").unwrap();
        let app = order.iter().position(|t| t == "app").unwrap();
        assert!(core < app, "core must build before app: {order:?}");
    }

    #[test]
    fn group_members_are_treated_as_build_order_predecessors() {
        let group = BuildTarget {
            id: "all".to_string(),
            config: Some(config("group", "member_ids: [core]")),
            ..Default::default()
        };
        let graph =
            BuildGraph::from_manifests(vec![manifest(vec![group, script_target("core", vec![])])])
                .unwrap();
        let order = graph.build_order("all").unwrap();
        assert!(
            order.iter().position(|t| t == "core").unwrap()
                < order.iter().position(|t| t == "all").unwrap()
        );
    }

    #[test]
    fn unknown_target_build_order_errors() {
        let graph =
            BuildGraph::from_manifests(vec![manifest(vec![script_target("a", vec![])])]).unwrap();
        assert!(matches!(
            graph.build_order("missing"),
            Err(BuildError::UnknownTarget(_))
        ));
    }

    #[test]
    fn action_waves_group_parallel_then_dependent() {
        // a, b independent; c consumes both outputs.
        let actions = vec![
            action("a", &[], &["a.out"]),
            action("b", &[], &["b.out"]),
            action("c", &["a.out", "b.out"], &["c.out"]),
        ];
        let waves = action_waves(&actions).unwrap();
        assert_eq!(waves.len(), 2);
        assert_eq!(waves[0], vec![0, 1]);
        assert_eq!(waves[1], vec![2]);
    }

    #[test]
    fn action_waves_detect_internal_cycle() {
        // Two actions each consuming the other's output.
        let actions = vec![
            action("x", &["y.out"], &["x.out"]),
            action("y", &["x.out"], &["y.out"]),
        ];
        assert!(matches!(action_waves(&actions), Err(BuildError::Cycle(_))));
    }

    #[test]
    fn global_waves_span_all_target_actions() {
        let graph = BuildGraph::from_manifests(vec![manifest(vec![BuildTarget {
            id: "fan".to_string(),
            actions: vec![
                action("a", &[], &["a.out"]),
                action("b", &[], &["b.out"]),
                action("c", &["a.out", "b.out"], &["c.out"]),
            ],
            ..Default::default()
        }])])
        .unwrap();
        let waves = graph.waves(&PluginRegistry::new()).unwrap();
        assert_eq!(waves.len(), 2);
        assert_eq!(waves[0].len(), 2);
        assert_eq!(waves[1].len(), 1);
    }
}

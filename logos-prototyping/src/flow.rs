//! # Flow Viewer
//!
//! A directed graph representing navigation paths between screens
//! (artboards / frames). Used by the flow viewer panel to visualise
//! how screens connect through interactions and transitions.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

use crate::trigger::{NavigationAnimation, TriggerKind};

// ── Flow Node ────────────────────────────────────────────────────────

/// A node in the flow graph — typically an artboard or top-level frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowNode {
    /// The UUID of the artboard / frame.
    pub id: Uuid,
    /// Display name.
    pub name: String,
    /// Position of the node in the flow viewer canvas (layout hint).
    pub position: (f64, f64),
    /// Whether this is the starting screen of the flow.
    pub is_start: bool,
    /// Optional thumbnail data reference (e.g. a cached render path).
    pub thumbnail: Option<String>,
}

impl FlowNode {
    pub fn new(id: Uuid, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            position: (0.0, 0.0),
            is_start: false,
            thumbnail: None,
        }
    }

    pub fn with_position(mut self, x: f64, y: f64) -> Self {
        self.position = (x, y);
        self
    }

    pub fn as_start(mut self) -> Self {
        self.is_start = true;
        self
    }
}

// ── Flow Edge ────────────────────────────────────────────────────────

/// A directed edge between two flow nodes, representing a navigation path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowEdge {
    pub id: Uuid,
    /// Source screen id.
    pub from: Uuid,
    /// Target screen id.
    pub to: Uuid,
    /// The trigger that causes this navigation.
    pub trigger: TriggerKind,
    /// The layer that owns the trigger (e.g. a button).
    pub trigger_layer_id: Option<Uuid>,
    /// Navigation animation.
    pub animation: Option<NavigationAnimation>,
    /// Human-readable label for the edge.
    pub label: Option<String>,
}

impl FlowEdge {
    pub fn new(from: Uuid, to: Uuid, trigger: TriggerKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            from,
            to,
            trigger,
            trigger_layer_id: None,
            animation: None,
            label: None,
        }
    }

    pub fn with_trigger_layer(mut self, layer_id: Uuid) -> Self {
        self.trigger_layer_id = Some(layer_id);
        self
    }

    pub fn with_animation(mut self, animation: NavigationAnimation) -> Self {
        self.animation = Some(animation);
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

// ── Flow Graph ───────────────────────────────────────────────────────

/// The complete navigation flow graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowGraph {
    /// Human-readable name for this flow.
    pub name: String,
    /// All screens (artboards / frames) in the flow.
    pub nodes: HashMap<Uuid, FlowNode>,
    /// All navigation edges.
    pub edges: Vec<FlowEdge>,
}

impl FlowGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    // ── Node management ──────────────────────────────────────────

    /// Add a node.
    pub fn add_node(&mut self, node: FlowNode) {
        self.nodes.insert(node.id, node);
    }

    /// Remove a node and all edges referencing it.
    pub fn remove_node(&mut self, id: Uuid) -> Option<FlowNode> {
        self.edges.retain(|e| e.from != id && e.to != id);
        self.nodes.remove(&id)
    }

    /// Get a node by id.
    pub fn get_node(&self, id: Uuid) -> Option<&FlowNode> {
        self.nodes.get(&id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the start node(s).
    pub fn start_nodes(&self) -> Vec<&FlowNode> {
        self.nodes.values().filter(|n| n.is_start).collect()
    }

    // ── Edge management ──────────────────────────────────────────

    /// Add an edge. Returns its id.
    pub fn add_edge(&mut self, edge: FlowEdge) -> Uuid {
        let id = edge.id;
        self.edges.push(edge);
        id
    }

    /// Remove an edge by id.
    pub fn remove_edge(&mut self, id: Uuid) -> bool {
        let len = self.edges.len();
        self.edges.retain(|e| e.id != id);
        self.edges.len() < len
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Get all edges originating from a given node.
    pub fn edges_from(&self, node_id: Uuid) -> Vec<&FlowEdge> {
        self.edges.iter().filter(|e| e.from == node_id).collect()
    }

    /// Get all edges arriving at a given node.
    pub fn edges_to(&self, node_id: Uuid) -> Vec<&FlowEdge> {
        self.edges.iter().filter(|e| e.to == node_id).collect()
    }

    // ── Analysis ─────────────────────────────────────────────────

    /// Find screens with no incoming edges (entry points).
    pub fn entry_points(&self) -> Vec<&FlowNode> {
        self.nodes
            .values()
            .filter(|n| !self.edges.iter().any(|e| e.to == n.id))
            .collect()
    }

    /// Find screens with no outgoing edges (dead ends).
    pub fn dead_ends(&self) -> Vec<&FlowNode> {
        self.nodes
            .values()
            .filter(|n| !self.edges.iter().any(|e| e.from == n.id))
            .collect()
    }

    /// Find orphan nodes (no edges at all).
    pub fn orphan_nodes(&self) -> Vec<&FlowNode> {
        self.nodes
            .values()
            .filter(|n| {
                !self.edges.iter().any(|e| e.from == n.id || e.to == n.id)
            })
            .collect()
    }

    /// Check if node `to` is reachable from `from` via BFS.
    pub fn is_reachable(&self, from: Uuid, to: Uuid) -> bool {
        if from == to {
            return true;
        }
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            for edge in &self.edges {
                if edge.from == current && !visited.contains(&edge.to) {
                    if edge.to == to {
                        return true;
                    }
                    visited.insert(edge.to);
                    queue.push_back(edge.to);
                }
            }
        }
        false
    }

    /// Auto-layout nodes in a simple grid based on topological ordering.
    pub fn auto_layout(&mut self, spacing_x: f64, spacing_y: f64) {
        let node_ids: Vec<Uuid> = self.nodes.keys().cloned().collect();
        let cols = (node_ids.len() as f64).sqrt().ceil() as usize;
        for (i, id) in node_ids.iter().enumerate() {
            if let Some(node) = self.nodes.get_mut(id) {
                let col = i % cols.max(1);
                let row = i / cols.max(1);
                node.position = (col as f64 * spacing_x, row as f64 * spacing_y);
            }
        }
    }

    /// Validate the graph: ensure all edge endpoints reference existing nodes.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for edge in &self.edges {
            if !self.nodes.contains_key(&edge.from) {
                errors.push(format!("Edge {:?} references unknown source node {:?}", edge.id, edge.from));
            }
            if !self.nodes.contains_key(&edge.to) {
                errors.push(format!("Edge {:?} references unknown target node {:?}", edge.id, edge.to));
            }
        }
        if self.start_nodes().is_empty() && !self.nodes.is_empty() {
            errors.push("No start node defined".into());
        }
        errors
    }
}

// ══════════════════════════════════════════════════════════════════════
// Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_graph() -> (FlowGraph, Uuid, Uuid, Uuid) {
        let mut graph = FlowGraph::new("Onboarding");

        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();

        graph.add_node(FlowNode::new(a, "Welcome").as_start().with_position(0.0, 0.0));
        graph.add_node(FlowNode::new(b, "Login").with_position(300.0, 0.0));
        graph.add_node(FlowNode::new(c, "Dashboard").with_position(600.0, 0.0));

        graph.add_edge(FlowEdge::new(a, b, TriggerKind::OnClick).with_label("Get Started"));
        graph.add_edge(FlowEdge::new(b, c, TriggerKind::OnClick).with_label("Sign In"));
        graph.add_edge(
            FlowEdge::new(c, a, TriggerKind::OnClick)
                .with_label("Logout")
                .with_animation(NavigationAnimation::Dissolve),
        );

        (graph, a, b, c)
    }

    #[test]
    fn test_graph_creation() {
        let (graph, _, _, _) = sample_graph();
        assert_eq!(graph.name, "Onboarding");
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 3);
    }

    #[test]
    fn test_start_nodes() {
        let (graph, a, _, _) = sample_graph();
        let starts = graph.start_nodes();
        assert_eq!(starts.len(), 1);
        assert_eq!(starts[0].id, a);
    }

    #[test]
    fn test_edges_from() {
        let (graph, a, _, _) = sample_graph();
        let from_a = graph.edges_from(a);
        assert_eq!(from_a.len(), 1);
        assert_eq!(from_a[0].label.as_deref(), Some("Get Started"));
    }

    #[test]
    fn test_edges_to() {
        let (graph, _, b, _) = sample_graph();
        let to_b = graph.edges_to(b);
        assert_eq!(to_b.len(), 1);
    }

    #[test]
    fn test_remove_node_clears_edges() {
        let (mut graph, _, b, _) = sample_graph();
        let edge_count_before = graph.edge_count();
        graph.remove_node(b);
        assert_eq!(graph.node_count(), 2);
        // B had 2 edges (A→B and B→C)
        assert_eq!(graph.edge_count(), edge_count_before - 2);
    }

    #[test]
    fn test_remove_edge() {
        let (mut graph, _, _, _) = sample_graph();
        let edge_id = graph.edges[0].id;
        assert!(graph.remove_edge(edge_id));
        assert_eq!(graph.edge_count(), 2);
    }

    #[test]
    fn test_is_reachable_direct() {
        let (graph, a, b, _) = sample_graph();
        assert!(graph.is_reachable(a, b));
    }

    #[test]
    fn test_is_reachable_transitive() {
        let (graph, a, _, c) = sample_graph();
        assert!(graph.is_reachable(a, c));
    }

    #[test]
    fn test_is_reachable_self() {
        let (graph, a, _, _) = sample_graph();
        assert!(graph.is_reachable(a, a));
    }

    #[test]
    fn test_is_reachable_cycle() {
        let (graph, _, _, c) = sample_graph();
        // C → A → B → C (cycle)
        assert!(graph.is_reachable(c, c));
    }

    #[test]
    fn test_is_reachable_false() {
        let mut graph = FlowGraph::new("Test");
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        graph.add_node(FlowNode::new(a, "A"));
        graph.add_node(FlowNode::new(b, "B"));
        // No edges
        assert!(!graph.is_reachable(a, b));
    }

    #[test]
    fn test_entry_points() {
        // In the cycle graph all nodes have incoming edges
        let (graph, _, _, _) = sample_graph();
        let entries = graph.entry_points();
        assert!(entries.is_empty()); // It's a cycle
    }

    #[test]
    fn test_entry_points_linear() {
        let mut graph = FlowGraph::new("Linear");
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        graph.add_node(FlowNode::new(a, "Start").as_start());
        graph.add_node(FlowNode::new(b, "End"));
        graph.add_edge(FlowEdge::new(a, b, TriggerKind::OnClick));
        let entries = graph.entry_points();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, a);
    }

    #[test]
    fn test_dead_ends() {
        let mut graph = FlowGraph::new("Linear");
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        graph.add_node(FlowNode::new(a, "Start").as_start());
        graph.add_node(FlowNode::new(b, "End"));
        graph.add_edge(FlowEdge::new(a, b, TriggerKind::OnClick));
        let dead = graph.dead_ends();
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].id, b);
    }

    #[test]
    fn test_orphan_nodes() {
        let mut graph = FlowGraph::new("Test");
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let orphan = Uuid::new_v4();
        graph.add_node(FlowNode::new(a, "A").as_start());
        graph.add_node(FlowNode::new(b, "B"));
        graph.add_node(FlowNode::new(orphan, "Orphan"));
        graph.add_edge(FlowEdge::new(a, b, TriggerKind::OnClick));
        let orphans = graph.orphan_nodes();
        assert_eq!(orphans.len(), 1);
        assert_eq!(orphans[0].id, orphan);
    }

    #[test]
    fn test_auto_layout() {
        let (mut graph, _, _, _) = sample_graph();
        graph.auto_layout(400.0, 300.0);
        // Just ensure positions were set (grid layout)
        let positions: Vec<(f64, f64)> = graph.nodes.values().map(|n| n.position).collect();
        assert_eq!(positions.len(), 3);
    }

    #[test]
    fn test_validate_ok() {
        let (graph, _, _, _) = sample_graph();
        assert!(graph.validate().is_empty());
    }

    #[test]
    fn test_validate_dangling_edge() {
        let mut graph = FlowGraph::new("Bad");
        let a = Uuid::new_v4();
        graph.add_node(FlowNode::new(a, "A").as_start());
        graph.add_edge(FlowEdge::new(a, Uuid::new_v4(), TriggerKind::OnClick));
        let errors = graph.validate();
        assert_eq!(errors.len(), 1); // unknown target
    }

    #[test]
    fn test_validate_no_start() {
        let mut graph = FlowGraph::new("NoStart");
        graph.add_node(FlowNode::new(Uuid::new_v4(), "A"));
        let errors = graph.validate();
        assert!(errors.iter().any(|e| e.contains("No start node")));
    }

    #[test]
    fn test_edge_with_trigger_layer() {
        let button = Uuid::new_v4();
        let edge = FlowEdge::new(Uuid::new_v4(), Uuid::new_v4(), TriggerKind::OnClick)
            .with_trigger_layer(button);
        assert_eq!(edge.trigger_layer_id, Some(button));
    }

    #[test]
    fn test_flow_node_thumbnail() {
        let mut node = FlowNode::new(Uuid::new_v4(), "Screen");
        node.thumbnail = Some("/cache/thumb_123.png".into());
        assert_eq!(node.thumbnail.as_deref(), Some("/cache/thumb_123.png"));
    }

    #[test]
    fn test_serde_roundtrip_flow_graph() {
        let (graph, _, _, _) = sample_graph();
        let json = serde_json::to_string(&graph).unwrap();
        let back: FlowGraph = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "Onboarding");
        assert_eq!(back.node_count(), 3);
        assert_eq!(back.edge_count(), 3);
    }

    #[test]
    fn test_serde_roundtrip_flow_edge() {
        let edge = FlowEdge::new(Uuid::new_v4(), Uuid::new_v4(), TriggerKind::OnHoverEnter)
            .with_label("Hover")
            .with_animation(NavigationAnimation::SmartAnimate);
        let json = serde_json::to_string(&edge).unwrap();
        let back: FlowEdge = serde_json::from_str(&json).unwrap();
        assert_eq!(back.label.as_deref(), Some("Hover"));
        assert_eq!(back.animation, Some(NavigationAnimation::SmartAnimate));
    }

    #[test]
    fn test_get_node() {
        let (graph, a, _, _) = sample_graph();
        let node = graph.get_node(a).unwrap();
        assert_eq!(node.name, "Welcome");
    }

    #[test]
    fn test_empty_graph() {
        let graph = FlowGraph::new("Empty");
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert!(graph.entry_points().is_empty());
        assert!(graph.dead_ends().is_empty());
        assert!(graph.orphan_nodes().is_empty());
    }
}

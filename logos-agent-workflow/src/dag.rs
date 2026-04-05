//! DAG definition — nodes, edges, topological sort, cycle detection.

use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

/// Errors from DAG operations.
#[derive(Debug, Error, PartialEq)]
pub enum DagError {
    #[error("node '{0}' already exists")]
    DuplicateNode(String),
    #[error("node '{0}' not found")]
    NodeNotFound(String),
    #[error("edge from '{0}' to '{1}' already exists")]
    DuplicateEdge(String, String),
    #[error("self-loop on node '{0}' is not allowed")]
    SelfLoop(String),
    #[error("workflow contains a cycle")]
    CycleDetected,
    #[error("workflow has no nodes")]
    EmptyWorkflow,
    #[error("workflow id is empty")]
    EmptyId,
}

/// The semantic kind of a workflow step.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Dispatches to a Logos agent.
    AgentTask,
    /// Runs a built-in transform (resize, export, etc.).
    Transform,
    /// A decision gate (fan-out to next steps based on condition).
    Decision,
    /// An external HTTP callback.
    Webhook,
    /// A no-op placeholder (useful for parallel join points).
    NoOp,
}

impl NodeKind {
    pub fn label(&self) -> &'static str {
        match self {
            NodeKind::AgentTask  => "AGENT_TASK",
            NodeKind::Transform  => "TRANSFORM",
            NodeKind::Decision   => "DECISION",
            NodeKind::Webhook    => "WEBHOOK",
            NodeKind::NoOp       => "NO_OP",
        }
    }
}

/// Runtime status of a step within an execution.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Pending,
    Running,
    Succeeded,
    Failed { reason: String },
    Skipped,
}

impl NodeStatus {
    pub fn is_terminal(&self) -> bool {
        matches!(self, NodeStatus::Succeeded | NodeStatus::Failed { .. } | NodeStatus::Skipped)
    }
    pub fn is_failed(&self) -> bool {
        matches!(self, NodeStatus::Failed { .. })
    }
}

/// A single step in a workflow DAG.
#[derive(Debug, Clone)]
pub struct StepNode {
    pub id:          String,
    pub kind:        NodeKind,
    pub description: String,
    /// Optional agent id this step is dispatched to.
    pub agent_id:    Option<String>,
    /// Whether this node can run in parallel with sibling nodes.
    pub parallelizable: bool,
}

impl StepNode {
    pub fn new(id: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: id.into(),
            kind,
            description: String::new(),
            agent_id: None,
            parallelizable: true,
        }
    }

    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = desc.into();
        self
    }

    pub fn with_agent(mut self, agent_id: impl Into<String>) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn sequential(mut self) -> Self {
        self.parallelizable = false;
        self
    }
}

/// A directed dependency edge from `from` → `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct StepEdge {
    pub from: String,
    pub to:   String,
    /// Optional label (e.g. "on_success", "on_error").
    pub label: Option<String>,
}

impl StepEdge {
    pub fn new(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self { from: from.into(), to: to.into(), label: None }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// A complete workflow definition (DAG of steps).
#[derive(Debug, Clone)]
pub struct WorkflowDef {
    pub id:    String,
    pub name:  String,
    nodes: HashMap<String, StepNode>,
    edges: Vec<StepEdge>,
}

impl WorkflowDef {
    /// Create a new empty workflow definition.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            nodes: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Add a step node. Returns error if duplicate.
    pub fn add_node(&mut self, node: StepNode) -> Result<(), DagError> {
        if self.nodes.contains_key(&node.id) {
            return Err(DagError::DuplicateNode(node.id));
        }
        self.nodes.insert(node.id.clone(), node);
        Ok(())
    }

    /// Add an edge. Validates both endpoints exist and no self-loops.
    pub fn add_edge(&mut self, edge: StepEdge) -> Result<(), DagError> {
        if edge.from == edge.to {
            return Err(DagError::SelfLoop(edge.from));
        }
        if !self.nodes.contains_key(&edge.from) {
            return Err(DagError::NodeNotFound(edge.from));
        }
        if !self.nodes.contains_key(&edge.to) {
            return Err(DagError::NodeNotFound(edge.to));
        }
        let exists = self.edges.iter()
            .any(|e| e.from == edge.from && e.to == edge.to);
        if exists {
            return Err(DagError::DuplicateEdge(edge.from, edge.to));
        }
        self.edges.push(edge);
        Ok(())
    }

    /// Compute a topological execution order using Kahn's algorithm.
    /// Returns `Err(CycleDetected)` if the graph has a cycle.
    pub fn topological_order(&self) -> Result<Vec<String>, DagError> {
        if self.nodes.is_empty() {
            return Err(DagError::EmptyWorkflow);
        }

        // Build in-degree map and adjacency list
        let mut in_degree: HashMap<&str, usize> = self.nodes.keys()
            .map(|id| (id.as_str(), 0))
            .collect();
        let mut adj: HashMap<&str, Vec<&str>> = self.nodes.keys()
            .map(|id| (id.as_str(), vec![]))
            .collect();

        for edge in &self.edges {
            *in_degree.entry(edge.to.as_str()).or_insert(0) += 1;
            adj.entry(edge.from.as_str()).or_default().push(edge.to.as_str());
        }

        // Start with all zero-in-degree nodes (sorted for determinism)
        let mut queue: VecDeque<&str> = {
            let mut starts: Vec<&str> = in_degree.iter()
                .filter_map(|(&id, &deg)| if deg == 0 { Some(id) } else { None })
                .collect();
            starts.sort();
            VecDeque::from(starts)
        };

        let mut order: Vec<String> = Vec::new();
        while let Some(node) = queue.pop_front() {
            order.push(node.to_owned());
            let mut nexts: Vec<&str> = adj[node].clone();
            nexts.sort();
            for next in nexts {
                let deg = in_degree.get_mut(next).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    queue.push_back(next);
                }
            }
        }

        if order.len() != self.nodes.len() {
            return Err(DagError::CycleDetected);
        }
        Ok(order)
    }

    /// Return the set of direct successors for a node.
    pub fn successors(&self, node_id: &str) -> Vec<&str> {
        self.edges.iter()
            .filter(|e| e.from == node_id)
            .map(|e| e.to.as_str())
            .collect()
    }

    /// Return the set of direct predecessors for a node.
    pub fn predecessors(&self, node_id: &str) -> Vec<&str> {
        self.edges.iter()
            .filter(|e| e.to == node_id)
            .map(|e| e.from.as_str())
            .collect()
    }

    /// Root nodes (no incoming edges).
    pub fn roots(&self) -> Vec<&str> {
        let has_predecessor: HashSet<&str> = self.edges.iter()
            .map(|e| e.to.as_str())
            .collect();
        let mut roots: Vec<&str> = self.nodes.keys()
            .filter(|id| !has_predecessor.contains(id.as_str()))
            .map(|id| id.as_str())
            .collect();
        roots.sort();
        roots
    }

    /// Leaf nodes (no outgoing edges).
    pub fn leaves(&self) -> Vec<&str> {
        let has_successor: HashSet<&str> = self.edges.iter()
            .map(|e| e.from.as_str())
            .collect();
        let mut leaves: Vec<&str> = self.nodes.keys()
            .filter(|id| !has_successor.contains(id.as_str()))
            .map(|id| id.as_str())
            .collect();
        leaves.sort();
        leaves
    }

    pub fn node_count(&self) -> usize { self.nodes.len() }
    pub fn edge_count(&self) -> usize { self.edges.len() }
    pub fn has_node(&self, id: &str) -> bool { self.nodes.contains_key(id) }
    pub fn get_node(&self, id: &str) -> Option<&StepNode> { self.nodes.get(id) }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn linear_workflow() -> WorkflowDef {
        let mut def = WorkflowDef::new("wf", "Linear");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        def.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
        def.add_node(StepNode::new("c", NodeKind::AgentTask)).unwrap();
        def.add_edge(StepEdge::new("a", "b")).unwrap();
        def.add_edge(StepEdge::new("b", "c")).unwrap();
        def
    }

    #[test]
    fn topological_order_linear() {
        let def = linear_workflow();
        let order = def.topological_order().unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn topological_order_diamond() {
        let mut def = WorkflowDef::new("wf", "Diamond");
        for id in &["start", "left", "right", "end"] {
            def.add_node(StepNode::new(*id, NodeKind::AgentTask)).unwrap();
        }
        def.add_edge(StepEdge::new("start", "left")).unwrap();
        def.add_edge(StepEdge::new("start", "right")).unwrap();
        def.add_edge(StepEdge::new("left",  "end")).unwrap();
        def.add_edge(StepEdge::new("right", "end")).unwrap();
        let order = def.topological_order().unwrap();
        assert_eq!(order[0], "start");
        assert_eq!(order[3], "end");
    }

    #[test]
    fn cycle_detected() {
        let mut def = WorkflowDef::new("wf", "Cyclic");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        def.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
        def.add_edge(StepEdge::new("a", "b")).unwrap();
        def.add_edge(StepEdge::new("b", "a")).unwrap();
        assert!(matches!(def.topological_order(), Err(DagError::CycleDetected)));
    }

    #[test]
    fn duplicate_node_rejected() {
        let mut def = WorkflowDef::new("wf", "W");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        assert!(matches!(
            def.add_node(StepNode::new("a", NodeKind::AgentTask)),
            Err(DagError::DuplicateNode(_))
        ));
    }

    #[test]
    fn unknown_edge_endpoint_rejected() {
        let mut def = WorkflowDef::new("wf", "W");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        assert!(matches!(
            def.add_edge(StepEdge::new("a", "ghost")),
            Err(DagError::NodeNotFound(_))
        ));
    }

    #[test]
    fn self_loop_rejected() {
        let mut def = WorkflowDef::new("wf", "W");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        assert!(matches!(
            def.add_edge(StepEdge::new("a", "a")),
            Err(DagError::SelfLoop(_))
        ));
    }

    #[test]
    fn duplicate_edge_rejected() {
        let mut def = WorkflowDef::new("wf", "W");
        def.add_node(StepNode::new("a", NodeKind::AgentTask)).unwrap();
        def.add_node(StepNode::new("b", NodeKind::AgentTask)).unwrap();
        def.add_edge(StepEdge::new("a", "b")).unwrap();
        assert!(matches!(
            def.add_edge(StepEdge::new("a", "b")),
            Err(DagError::DuplicateEdge(_, _))
        ));
    }

    #[test]
    fn roots_and_leaves() {
        let def = linear_workflow();
        assert_eq!(def.roots(),  vec!["a"]);
        assert_eq!(def.leaves(), vec!["c"]);
    }

    #[test]
    fn successors_and_predecessors() {
        let def = linear_workflow();
        assert_eq!(def.successors("b"),   vec!["c"]);
        assert_eq!(def.predecessors("b"), vec!["a"]);
    }

    #[test]
    fn node_kind_labels() {
        assert_eq!(NodeKind::AgentTask.label(), "AGENT_TASK");
        assert_eq!(NodeKind::Decision.label(),  "DECISION");
        assert_eq!(NodeKind::Webhook.label(),   "WEBHOOK");
    }

    #[test]
    fn node_status_terminal_flags() {
        assert!(NodeStatus::Succeeded.is_terminal());
        assert!(NodeStatus::Failed { reason: "x".into() }.is_failed());
        assert!(!NodeStatus::Running.is_terminal());
    }

    #[test]
    fn empty_workflow_order_errors() {
        let def = WorkflowDef::new("wf", "Empty");
        assert!(matches!(def.topological_order(), Err(DagError::EmptyWorkflow)));
    }
}

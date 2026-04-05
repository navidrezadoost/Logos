//! # logos-agent-workflow — Agent Workflow Orchestrator
//!
//! DAG-based workflow execution engine with retry/timeout/backoff policies
//! and checkpointing for Logos agent pipelines.
//!
//! ## Quick start
//!
//! ```rust
//! use logos_agent_workflow::{
//!     WorkflowDef, StepNode, StepEdge, NodeKind,
//!     RetryPolicy, TimeoutPolicy,
//!     WorkflowExecutor, StepOutcome,
//!     CheckpointStore,
//! };
//!
//! // Build a two-step workflow: generate → export
//! let mut def = WorkflowDef::new("wf-1", "Generate & Export");
//! def.add_node(StepNode::new("gen",    NodeKind::AgentTask)).unwrap();
//! def.add_node(StepNode::new("export", NodeKind::AgentTask)).unwrap();
//! def.add_edge(StepEdge::new("gen", "export")).unwrap();
//!
//! // Validate and get execution order
//! let order = def.topological_order().unwrap();
//! assert_eq!(order[0], "gen");
//! assert_eq!(order[1], "export");
//!
//! // Set up policies on a node
//! let retry = RetryPolicy::exponential(3, 100);
//! assert_eq!(retry.max_attempts, 3);
//!
//! // Create executor and run workflow
//! let store = CheckpointStore::new();
//! let mut exec = WorkflowExecutor::new(store);
//! let result = exec.run(&def, |node_id| {
//!     StepOutcome::success(node_id, 10)
//! });
//! assert!(result.is_ok());
//! ```

pub mod dag;
pub mod policy;
pub mod executor;
pub mod checkpoint;

pub use dag::{WorkflowDef, StepNode, StepEdge, NodeKind, NodeStatus, DagError};
pub use policy::{RetryPolicy, TimeoutPolicy, BackoffPolicy, BackoffKind, PolicyError};
pub use executor::{WorkflowExecutor, WorkflowRun, StepOutcome, RunStatus, ExecutorError};
pub use checkpoint::{CheckpointStore, WorkflowCheckpoint, StepCheckpoint, CheckpointError};

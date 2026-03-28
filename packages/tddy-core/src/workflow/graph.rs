//! Graph and GraphBuilder — task registry and edges.
//!
//! Mirrors [graph-flow graph.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/graph.rs).

use crate::workflow::context::Context;
use crate::workflow::task::Task;
use std::collections::HashMap;
use std::sync::Arc;

/// Hook-triggered elicitation: the workflow should pause for user interaction.
/// Defined at the workflow layer to avoid coupling with presenter types.
#[derive(Debug, Clone)]
pub enum ElicitationEvent {
    /// User must approve or revise session document content before continuing.
    DocumentApproval { content: String },
    /// Daemon mode: agent suggested branch/worktree names; client confirms before worktree creation.
    WorktreeConfirmation {
        suggested_branch: String,
        suggested_worktree: String,
    },
}

/// Execution status after a step.
#[derive(Debug, Clone)]
pub enum ExecutionStatus {
    Paused {
        message: Option<String>,
    },
    WaitingForInput {
        message: Option<String>,
    },
    /// Hook requested user elicitation; caller must handle event and resume.
    ElicitationNeeded {
        event: ElicitationEvent,
    },
    Completed,
    Error(String),
}

/// Result of executing one step.
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub session_id: String,
    pub current_task_id: Option<String>,
}

/// Graph of tasks and edges.
pub struct Graph {
    pub id: String,
    tasks: HashMap<String, Arc<dyn Task>>,
    edges: Vec<(String, String)>,
    conditional_edges: Vec<ConditionalEdge>,
}

struct ConditionalEdge {
    from: String,
    condition: Box<dyn Fn(&Context) -> bool + Send + Sync>,
    when_true: String,
    when_false: String,
}

impl Graph {
    pub fn get_task(&self, id: &str) -> Option<Arc<dyn Task>> {
        self.tasks.get(id).cloned()
    }

    pub fn next_task_id(&self, from: &str, context: &Context) -> Option<String> {
        for ce in &self.conditional_edges {
            if ce.from == from {
                let next = if (ce.condition)(context) {
                    ce.when_true.clone()
                } else {
                    ce.when_false.clone()
                };
                return self.tasks.contains_key(&next).then_some(next);
            }
        }
        self.edges
            .iter()
            .find(|(f, _)| f == from)
            .map(|(_, t)| t.clone())
    }

    pub fn task_ids(&self) -> impl Iterator<Item = &String> {
        self.tasks.keys()
    }
}

/// Fluent builder for Graph.
pub struct GraphBuilder {
    id: String,
    tasks: HashMap<String, Arc<dyn Task>>,
    edges: Vec<(String, String)>,
    conditional_edges: Vec<ConditionalEdge>,
}

impl GraphBuilder {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            tasks: HashMap::new(),
            edges: Vec::new(),
            conditional_edges: Vec::new(),
        }
    }

    pub fn add_task(mut self, task: Arc<dyn Task>) -> Self {
        self.tasks.insert(task.id().to_string(), task);
        self
    }

    pub fn add_edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push((from.into(), to.into()));
        self
    }

    pub fn add_conditional_edge<F>(
        mut self,
        from: impl Into<String>,
        condition: F,
        when_true: impl Into<String>,
        when_false: impl Into<String>,
    ) -> Self
    where
        F: Fn(&Context) -> bool + Send + Sync + 'static,
    {
        self.conditional_edges.push(ConditionalEdge {
            from: from.into(),
            condition: Box::new(condition),
            when_true: when_true.into(),
            when_false: when_false.into(),
        });
        self
    }

    pub fn build(self) -> Graph {
        Graph {
            id: self.id,
            tasks: self.tasks,
            edges: self.edges,
            conditional_edges: self.conditional_edges,
        }
    }
}

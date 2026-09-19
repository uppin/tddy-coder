//! Semantic newtypes for workflow goals vs persisted state strings.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;

/// Identifies a workflow goal / graph task (e.g. `"plan"`, `"red"`, `"evaluate"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GoalId(String);

impl GoalId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for GoalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for GoalId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for GoalId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for GoalId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for GoalId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Persisted workflow state label (e.g. `"Init"`, `"Planned"`, `"GreenComplete"`).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct WorkflowState(String);

impl WorkflowState {
    pub fn new(state: impl Into<String>) -> Self {
        Self(state.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl AsRef<str> for WorkflowState {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Deref for WorkflowState {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<&str> for WorkflowState {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for WorkflowState {
    fn from(s: String) -> Self {
        Self(s)
    }
}

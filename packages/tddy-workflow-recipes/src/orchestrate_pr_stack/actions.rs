//! Custom orchestrator tasks: SpawnTask, MergeTask, RepointTask.

use async_trait::async_trait;
use tddy_core::workflow::context::Context;
use tddy_core::workflow::task::{Task, TaskResult};

/// Task: spawn child sessions for nodes that are NotSpawned.
pub struct SpawnTask {}

impl SpawnTask {
    pub fn new() -> Self { Self {} }
}

impl Default for SpawnTask {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Task for SpawnTask {
    fn id(&self) -> &str { "spawn" }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!("SpawnTask::run: not yet implemented")
    }
}

/// Task: merge the next ready PR via GitHub REST API.
pub struct MergeTask {}

impl MergeTask {
    pub fn new() -> Self { Self {} }
}

impl Default for MergeTask {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Task for MergeTask {
    fn id(&self) -> &str { "merge" }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!("MergeTask::run: not yet implemented")
    }
}

/// Task: repoint dependent PRs after a parent has been merged.
pub struct RepointTask {}

impl RepointTask {
    pub fn new() -> Self { Self {} }
}

impl Default for RepointTask {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl Task for RepointTask {
    fn id(&self) -> &str { "repoint" }

    async fn run(
        &self,
        _context: Context,
    ) -> Result<TaskResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!("RepointTask::run: not yet implemented")
    }
}

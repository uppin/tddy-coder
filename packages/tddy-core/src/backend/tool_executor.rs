//! Tool executor abstraction for submitting structured results.
//!
//! StubBackend uses InMemoryToolExecutor (tests and tddy-demo): stores via `store_submit_result`.
//! ProcessToolExecutor (for real agents) runs `tddy-tools submit`; not used by StubBackend.

use crate::toolcall::SubmitResultChannel;
use std::path::Path;

/// Executes a tool call to submit structured JSON for a goal.
/// Implementations either store in-memory (tests) or invoke tddy-tools (demo).
pub trait ToolExecutor: Send + Sync {
    /// Submit JSON data for the given goal. The workflow reads it via `take_submit_result_for_goal`.
    fn submit(
        &self,
        goal: &str,
        json: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

/// In-memory executor for tests. Stores to a per-instance channel (no global state).
#[derive(Debug, Clone)]
pub struct InMemoryToolExecutor {
    channel: SubmitResultChannel,
}

impl Default for InMemoryToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryToolExecutor {
    pub fn new() -> Self {
        Self {
            channel: SubmitResultChannel::new(),
        }
    }

    pub fn channel(&self) -> &SubmitResultChannel {
        &self.channel
    }
}

impl ToolExecutor for InMemoryToolExecutor {
    fn submit(
        &self,
        goal: &str,
        json: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!(
            "[tool_executor] InMemory: storing goal={} json_len={}",
            goal,
            json.len()
        );
        self.channel.store(goal, json);
        Ok(())
    }
}

/// Process executor for tddy-demo. Spawns `tddy-tools submit` which connects to the presenter socket.
#[derive(Debug, Clone)]
pub struct ProcessToolExecutor {
    socket_path: std::path::PathBuf,
    working_dir: std::path::PathBuf,
}

impl ProcessToolExecutor {
    pub fn new(socket_path: impl AsRef<Path>, working_dir: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            working_dir: working_dir.as_ref().to_path_buf(),
        }
    }
}

impl ToolExecutor for ProcessToolExecutor {
    fn submit(
        &self,
        goal: &str,
        json: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        log::debug!(
            "[tool_executor] Process: goal={} socket={} cwd={}",
            goal,
            self.socket_path.display(),
            self.working_dir.display()
        );
        let output = std::process::Command::new("tddy-tools")
            .args(["submit", "--goal", goal, "--data", json])
            .env("TDDY_SOCKET", &self.socket_path)
            .current_dir(&self.working_dir)
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!(
                "[tool_executor] Process: tddy-tools submit failed stderr={}",
                stderr
            );
            return Err(format!("tddy-tools submit failed: {}", stderr).into());
        }
        log::debug!(
            "[tool_executor] Process: tddy-tools submit succeeded for goal={}",
            goal
        );
        Ok(())
    }
}

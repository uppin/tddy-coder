//! FlowRunner — load session, execute one step, save session.
//!
//! Mirrors [graph-flow runner.rs](https://github.com/a-agmon/rs-graph-llm/blob/main/graph-flow/src/runner.rs).

use crate::graph::{ExecutionResult, ExecutionStatus, Graph};
use crate::hooks::RunnerHooks;
use crate::session::SessionStorage;
use crate::task::NextAction;
use std::sync::Arc;

/// Orchestrates load → execute → save cycle for workflow sessions.
pub struct FlowRunner {
    graph: Arc<Graph>,
    storage: Arc<dyn SessionStorage>,
    hooks: Option<Arc<dyn RunnerHooks>>,
}

impl FlowRunner {
    pub fn new(graph: Arc<Graph>, storage: Arc<dyn SessionStorage>) -> Self {
        Self {
            graph,
            storage,
            hooks: None,
        }
    }

    pub fn new_with_hooks(
        graph: Arc<Graph>,
        storage: Arc<dyn SessionStorage>,
        hooks: Option<Arc<dyn RunnerHooks>>,
    ) -> Self {
        Self {
            graph,
            storage,
            hooks,
        }
    }

    pub async fn run(
        &self,
        session_id: &str,
    ) -> Result<ExecutionResult, Box<dyn std::error::Error + Send + Sync>> {
        let session = self
            .storage
            .get(session_id)
            .await?
            .ok_or("Session not found")?;

        let task = self
            .graph
            .get_task(&session.current_task_id)
            .ok_or("Task not found")?;

        let ctx = session.context.clone();
        ctx.set_sync("workflow_engine_graph_id", session.graph_id.clone());
        ctx.set_sync(
            "workflow_engine_current_task_id",
            session.current_task_id.clone(),
        );

        if let Some(ref hooks) = self.hooks {
            hooks.before_task(&session.current_task_id, &ctx)?;
            hooks.on_enter_task(&session.current_task_id, &ctx);
        }

        let result = match task.run(ctx.clone()).await {
            Ok(r) => r,
            Err(e) => {
                if let Some(ref hooks) = self.hooks {
                    hooks.on_exit_task(&session.current_task_id, &ctx);
                    hooks.on_error(&session.current_task_id, &ctx, e.as_ref());
                }
                return Err(e);
            }
        };

        if let Some(ref hooks) = self.hooks {
            hooks.on_exit_task(&session.current_task_id, &ctx);
        }

        let next_task_id = match &result.next_action {
            NextAction::WaitForInput => {
                let mut session = session;
                session.status_message = result.status_message.clone();
                self.storage.save(&session).await?;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::WaitingForInput {
                        message: result.status_message,
                    },
                    session_id: session_id.to_string(),
                    current_task_id: Some(session.current_task_id),
                });
            }
            NextAction::End => {
                if let Some(ref hooks) = self.hooks {
                    hooks.after_task(&session.current_task_id, &ctx, &result)?;
                }
                let mut session = session;
                session.status_message = result.status_message.clone();
                self.storage.save(&session).await?;
                return Ok(ExecutionResult {
                    status: ExecutionStatus::Completed,
                    session_id: session_id.to_string(),
                    current_task_id: None,
                });
            }
            NextAction::Continue | NextAction::ContinueAndExecute => {
                match self.graph.next_task_id(&session.current_task_id, &ctx) {
                    Some(next) => next,
                    None => {
                        let mut session = session;
                        session.status_message = result.status_message.clone();
                        self.storage.save(&session).await?;
                        return Ok(ExecutionResult {
                            status: ExecutionStatus::WaitingForInput {
                                message: result.status_message,
                            },
                            session_id: session_id.to_string(),
                            current_task_id: Some(session.current_task_id),
                        });
                    }
                }
            }
            NextAction::GoTo(id) => id.clone(),
            NextAction::GoBack => return Err("GoBack not implemented".into()),
        };

        if let Some(ref hooks) = self.hooks {
            hooks.after_task(&session.current_task_id, &ctx, &result)?;
        }

        let elicitation = self
            .hooks
            .as_ref()
            .and_then(|h| h.elicitation_after_task(&session.current_task_id, &ctx, &result));

        let mut session = session;
        session.current_task_id = next_task_id.clone();
        session.status_message = result.status_message;
        self.storage.save(&session).await?;

        let status = match elicitation {
            Some(event) => ExecutionStatus::ElicitationNeeded { event },
            None => ExecutionStatus::Paused {
                message: Some("Step complete".to_string()),
            },
        };

        Ok(ExecutionResult {
            status,
            session_id: session_id.to_string(),
            current_task_id: Some(next_task_id),
        })
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests: `FlowRunner` lifecycle hooks fire on every execution path.
    //!
    //! Feature: docs/ft/coder/discovery-agent.md (Phase A criteria 5)
    //! Changeset: docs/dev/1-WIP/2026-06-24-changeset-tddy-graph-extraction.md

    use std::error::Error;
    use std::sync::{Arc, Mutex};

    use crate::context::Context;
    use crate::graph::{ExecutionStatus, GraphBuilder};
    use crate::hooks::RunnerHooks;
    use crate::session::FileSessionStorage;
    use crate::task::{EchoTask, EndTask, FailingTask, TaskResult, WaitingTask};

    use super::FlowRunner;

    /// Test double: records which lifecycle-hook calls were made and in what order.
    #[derive(Debug)]
    struct RecordingHooks {
        log: Mutex<Vec<String>>,
    }

    impl RecordingHooks {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                log: Mutex::new(Vec::new()),
            })
        }

        fn entries(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl RunnerHooks for RecordingHooks {
        fn before_task(
            &self,
            task_id: &str,
            _: &Context,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.log.lock().unwrap().push(format!("before:{task_id}"));
            Ok(())
        }

        fn after_task(
            &self,
            task_id: &str,
            _: &Context,
            _: &TaskResult,
        ) -> Result<(), Box<dyn Error + Send + Sync>> {
            self.log.lock().unwrap().push(format!("after:{task_id}"));
            Ok(())
        }

        fn on_error(&self, task_id: &str, _: &Context, _: &(dyn Error + Send + Sync)) {
            self.log.lock().unwrap().push(format!("on_error:{task_id}"));
        }

        fn on_enter_task(&self, task_id: &str, _: &Context) {
            self.log.lock().unwrap().push(format!("on_enter:{task_id}"));
        }

        fn on_exit_task(&self, task_id: &str, _: &Context) {
            self.log.lock().unwrap().push(format!("on_exit:{task_id}"));
        }
    }

    fn temp_storage() -> Arc<FileSessionStorage> {
        let dir =
            std::env::temp_dir().join(format!("tddy-graph-runner-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        Arc::new(FileSessionStorage::new(dir))
    }

    async fn make_runner_with_hooks(
        graph: Arc<crate::graph::Graph>,
        hooks: Arc<RecordingHooks>,
    ) -> (FlowRunner, String) {
        use crate::session::SessionStorage;
        let storage = temp_storage();
        let first_task_id = graph.task_ids().next().unwrap().clone();
        let graph_id = graph.id.clone();
        let session_id = uuid::Uuid::new_v4().to_string();
        let session =
            crate::session::Session::new_from_task(session_id.clone(), graph_id, first_task_id);
        storage.save(&session).await.unwrap();
        let runner = FlowRunner::new_with_hooks(
            graph,
            storage as Arc<dyn crate::session::SessionStorage>,
            Some(hooks as Arc<dyn RunnerHooks>),
        );
        (runner, session_id)
    }

    /// `tddy-graph` is a standalone crate: `FlowRunner` can run a graph built from `tddy-graph`
    /// types alone, with no `tddy-core` dependency.
    #[tokio::test]
    async fn flow_runner_runs_a_graph_built_from_tddy_graph_types() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("standalone-graph")
                .add_task(Arc::new(EndTask::new("finish")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        let result = runner.run(&session_id).await.expect("runner must succeed");

        // Then
        assert!(
            matches!(result.status, ExecutionStatus::Completed),
            "a graph with only an EndTask must complete; got {:?}",
            result.status
        );
    }

    /// `on_enter_task` fires immediately before `task.run`.
    #[tokio::test]
    async fn on_enter_task_fires_before_task_run() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("enter-test")
                .add_task(Arc::new(EndTask::new("step")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        runner.run(&session_id).await.expect("runner must succeed");

        // Then
        let log = hooks.entries();
        let enter_pos = log
            .iter()
            .position(|e| e == "on_enter:step")
            .expect("on_enter must be recorded");
        let after_pos = log
            .iter()
            .position(|e| e == "after:step")
            .unwrap_or_else(|| panic!("after_task must be recorded for EndTask; log: {log:?}"));
        assert!(
            enter_pos < after_pos,
            "on_enter_task must fire before after_task; log: {log:?}"
        );
    }

    /// `on_exit_task` fires after a successful step that produces `Continue`.
    #[tokio::test]
    async fn on_exit_task_fires_after_a_successful_continue_step() {
        // Given — a two-node graph: EchoTask (Continue) → EndTask
        let graph = Arc::new(
            GraphBuilder::new("exit-continue-test")
                .add_task(Arc::new(EchoTask::new("echo")))
                .add_task(Arc::new(EndTask::new("end")))
                .add_edge("echo", "end")
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        runner.run(&session_id).await.ok();

        // Then — on_exit fires after the echo step
        let log = hooks.entries();
        assert!(
            log.contains(&"on_exit:echo".to_string()),
            "on_exit_task must fire after a successful Continue step; log: {log:?}"
        );
    }

    /// `on_exit_task` fires in the task error arm (before `on_error`) AND the error is propagated.
    #[tokio::test]
    async fn on_exit_task_fires_when_task_returns_error() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("exit-error-test")
                .add_task(Arc::new(FailingTask::new("fail")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        let result = runner.run(&session_id).await;

        // Then — error is propagated
        assert!(result.is_err(), "FailingTask must cause run to return Err");

        // And — on_exit fires before on_error, both are recorded
        let log = hooks.entries();
        let exit_pos = log
            .iter()
            .position(|e| e == "on_exit:fail")
            .expect("on_exit_task must fire in the error arm; log: {log:?}");
        let error_pos = log
            .iter()
            .position(|e| e == "on_error:fail")
            .expect("on_error must fire after a task error; log: {log:?}");
        assert!(
            exit_pos < error_pos,
            "on_exit_task must fire before on_error (consistent with clear_sinks before on_error); log: {log:?}"
        );
    }

    /// `on_exit_task` fires even when the task returns `WaitForInput` (early return path).
    /// This mirrors the current `clear_sinks` at runner.rs:84 — unconditional, before the
    /// `next_action` match that catches `WaitForInput`.
    #[tokio::test]
    async fn on_exit_task_fires_on_wait_for_input_early_return() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("exit-wait-test")
                .add_task(Arc::new(WaitingTask::new("wait")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        let result = runner
            .run(&session_id)
            .await
            .expect("WaitForInput must not be an Err");

        // Then — runner returns WaitingForInput
        assert!(
            matches!(result.status, ExecutionStatus::WaitingForInput { .. }),
            "WaitingTask must produce WaitingForInput; got {:?}",
            result.status
        );

        // And — on_exit was recorded despite the early return
        let log = hooks.entries();
        assert!(
            log.contains(&"on_exit:wait".to_string()),
            "on_exit_task must fire before the WaitForInput early return; log: {log:?}"
        );
    }

    /// `on_exit_task` fires when the task returns `End` (early return path).
    #[tokio::test]
    async fn on_exit_task_fires_on_end_early_return() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("exit-end-test")
                .add_task(Arc::new(EndTask::new("finish")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        let result = runner
            .run(&session_id)
            .await
            .expect("EndTask must not be an Err");

        // Then
        assert!(
            matches!(result.status, ExecutionStatus::Completed),
            "EndTask must complete; got {:?}",
            result.status
        );
        let log = hooks.entries();
        assert!(
            log.contains(&"on_exit:finish".to_string()),
            "on_exit_task must fire before the End early return; log: {log:?}"
        );
    }

    /// `on_exit_task` fires when `Continue` has no successor (treated like WaitForInput).
    #[tokio::test]
    async fn on_exit_task_fires_when_continue_has_no_successor() {
        // Given — single node, no outgoing edge; EchoTask returns Continue
        let graph = Arc::new(
            GraphBuilder::new("exit-no-successor-test")
                .add_task(Arc::new(EchoTask::new("solo")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        let result = runner
            .run(&session_id)
            .await
            .expect("no-successor must not be an Err");

        // Then — runner pauses (WaitingForInput) since there's no next node
        assert!(
            matches!(result.status, ExecutionStatus::WaitingForInput { .. }),
            "Continue with no successor must produce WaitingForInput; got {:?}",
            result.status
        );
        let log = hooks.entries();
        assert!(
            log.contains(&"on_exit:solo".to_string()),
            "on_exit_task must fire before the no-successor pause path; log: {log:?}"
        );
    }

    /// `on_enter_task` and `on_exit_task` each fire exactly once per `runner.run` call.
    #[tokio::test]
    async fn on_enter_and_on_exit_fire_exactly_once_per_step() {
        // Given
        let graph = Arc::new(
            GraphBuilder::new("exactly-once-test")
                .add_task(Arc::new(EndTask::new("once")))
                .build(),
        );
        let hooks = RecordingHooks::new();
        let (runner, session_id) = make_runner_with_hooks(graph, hooks.clone()).await;

        // When
        runner.run(&session_id).await.expect("runner must succeed");

        // Then
        let log = hooks.entries();
        let enter_count = log.iter().filter(|e| e.starts_with("on_enter:")).count();
        let exit_count = log.iter().filter(|e| e.starts_with("on_exit:")).count();
        assert_eq!(
            enter_count, 1,
            "on_enter_task must fire exactly once per run; log: {log:?}"
        );
        assert_eq!(
            exit_count, 1,
            "on_exit_task must fire exactly once per run; log: {log:?}"
        );
    }
}

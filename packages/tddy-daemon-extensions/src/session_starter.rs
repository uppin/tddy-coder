//! Starting a session on behalf of an operator the daemon has already identified, as a port.
//!
//! A satellite that takes commands from an operator — Telegram is the first — must start sessions
//! the way a web client does: through the one generic start, with the spec launch, the hooks, the
//! provider route, tmux and the start's own branch-conflict guard. It cannot name the session host
//! (no session crate may name a satellite, and the satellite is built before the host), so it holds
//! this port instead.
//!
//! The operator is keyed by the **OS user** the daemon maps them to — the satellite has already
//! authenticated them its own way (Telegram: the chat's linked GitHub login) — so no session token
//! is minted inside the daemon, and none reaches the agent's environment.
//!
//! A daemon wires the host's implementation through [`LateBoundSessionStarter`] once the host is
//! built; until then, and on a daemon that never binds one, every start is refused
//! `FAILED_PRECONDITION` rather than waited on.

use std::sync::{Arc, OnceLock};

use tddy_rpc::Status;

/// A session start for an operator the caller has already identified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSessionStart {
    /// The OS user the session runs as — the operator's mapped user.
    pub os_user: String,
    /// The registered project the session works in.
    pub project_id: String,
    /// The session type to start: a CLI agent's `<id>-cli`.
    pub session_type: String,
    /// The model the operator chose; empty for an agent that takes none.
    pub model: String,
    /// The first prompt; empty for none.
    pub initial_prompt: String,
    /// Where the session's worktree branches from.
    pub branch: OperatorBranchChoice,
}

/// The branch an operator's session works on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorBranchChoice {
    /// A new branch `new_branch_name` cut from `integration_base_ref` (empty: the project's
    /// default base). An empty name asks the start to generate one, which an agent declaring
    /// `branch_name_required` refuses.
    NewBranchFromBase {
        new_branch_name: String,
        integration_base_ref: String,
    },
    /// An existing branch, worked on in place.
    WorkOnBranch { branch: String },
}

/// What an operator start ended in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorStartOutcome {
    /// The session is running.
    Started { session_id: String },
    /// Another session already owns the branch; nothing was created. The operator chooses how to
    /// continue (`docs/ft/daemon/session-branch-conflict.md`).
    BranchConflict(OperatorBranchConflict),
}

/// The branch another session holds, and the suffixed name a retry may take instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorBranchConflict {
    pub branch: String,
    pub owner_session_id: String,
    pub suggested_branch_name: String,
}

/// The daemon's generic session start, offered to a satellite.
#[async_trait::async_trait]
pub trait SessionStarter: Send + Sync {
    /// Start `start` as its OS user. A start onto a branch another session holds is not created
    /// and reports [`OperatorStartOutcome::BranchConflict`]; every refusal of the generic start
    /// (no coding agents, an unknown session type, a model the agent requires) is its `Status`.
    async fn start_for_operator(
        &self,
        start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, Status>;
}

/// No session host wired: every start is refused `FAILED_PRECONDITION`.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoSessionStarter;

#[async_trait::async_trait]
impl SessionStarter for NoSessionStarter {
    async fn start_for_operator(
        &self,
        start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, Status> {
        Err(not_bound(&start))
    }
}

/// A [`SessionStarter`] bound once, after the host it forwards to exists. Starts before the bind
/// are refused `FAILED_PRECONDITION` — never queued, never waited on.
#[derive(Default)]
pub struct LateBoundSessionStarter {
    bound: OnceLock<Arc<dyn SessionStarter>>,
}

impl LateBoundSessionStarter {
    /// A starter with nothing bound yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Bind the host's starter. A second bind is refused, the first kept: a daemon has one host.
    pub fn bind(&self, starter: Arc<dyn SessionStarter>) -> Result<(), Status> {
        // TODO(telegram-spec-launch): implement
        let _ = starter;
        todo!("telegram-spec-launch: LateBoundSessionStarter::bind")
    }
}

#[async_trait::async_trait]
impl SessionStarter for LateBoundSessionStarter {
    async fn start_for_operator(
        &self,
        start: OperatorSessionStart,
    ) -> Result<OperatorStartOutcome, Status> {
        // TODO(telegram-spec-launch): implement — forward to the bound starter; refuse with
        // `not_bound` before the bind.
        let _ = (&self.bound, start);
        todo!("telegram-spec-launch: LateBoundSessionStarter::start_for_operator")
    }
}

fn not_bound(start: &OperatorSessionStart) -> Status {
    Status::failed_precondition(format!(
        "no session host is bound on this daemon; cannot start a {} session",
        start.session_type
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use pretty_assertions::assert_eq;
    use tddy_rpc::Code;

    use super::*;

    /// A starter that records each start it is handed and answers it started as `session_id`.
    struct ARecordingStarter {
        session_id: &'static str,
        received: Mutex<Vec<OperatorSessionStart>>,
    }

    fn a_recording_starter(session_id: &'static str) -> Arc<ARecordingStarter> {
        Arc::new(ARecordingStarter {
            session_id,
            received: Mutex::new(Vec::new()),
        })
    }

    impl ARecordingStarter {
        fn received(&self) -> Vec<OperatorSessionStart> {
            self.received.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl SessionStarter for ARecordingStarter {
        async fn start_for_operator(
            &self,
            start: OperatorSessionStart,
        ) -> Result<OperatorStartOutcome, Status> {
            self.received.lock().unwrap().push(start);
            Ok(OperatorStartOutcome::Started {
                session_id: self.session_id.to_string(),
            })
        }
    }

    fn a_claude_cli_start() -> OperatorSessionStart {
        OperatorSessionStart {
            os_user: "dev".to_string(),
            project_id: "proj".to_string(),
            session_type: "claude-cli".to_string(),
            model: "opus".to_string(),
            initial_prompt: "fix the bug".to_string(),
            branch: OperatorBranchChoice::NewBranchFromBase {
                new_branch_name: "feature/fix-the-bug".to_string(),
                integration_base_ref: String::new(),
            },
        }
    }

    #[tokio::test]
    async fn a_start_before_the_bind_is_refused_failed_precondition_naming_its_session_type() {
        // Given
        let starter = LateBoundSessionStarter::new();

        // When
        let refused = starter
            .start_for_operator(a_claude_cli_start())
            .await
            .map_err(|status| (status.code, status.message));

        // Then
        assert_eq!(
            refused,
            Err((
                Code::FailedPrecondition,
                "no session host is bound on this daemon; cannot start a claude-cli session"
                    .to_string()
            ))
        );
    }

    #[tokio::test]
    async fn a_start_after_the_bind_reaches_the_bound_starter_unchanged() {
        // Given
        let host = a_recording_starter("started-1");
        let starter = LateBoundSessionStarter::new();
        starter
            .bind(Arc::clone(&host) as Arc<dyn SessionStarter>)
            .expect("the first bind is accepted");

        // When
        let outcome = starter
            .start_for_operator(a_claude_cli_start())
            .await
            .map_err(|status| status.code);

        // Then
        assert_eq!(
            (outcome, host.received()),
            (
                Ok(OperatorStartOutcome::Started {
                    session_id: "started-1".to_string()
                }),
                vec![a_claude_cli_start()]
            )
        );
    }

    #[tokio::test]
    async fn a_second_bind_is_refused_and_the_first_starter_keeps_serving() {
        // Given
        let first = a_recording_starter("from-the-first");
        let second = a_recording_starter("from-the-second");
        let starter = LateBoundSessionStarter::new();
        starter
            .bind(Arc::clone(&first) as Arc<dyn SessionStarter>)
            .expect("the first bind is accepted");

        // When
        let rebind = starter
            .bind(Arc::clone(&second) as Arc<dyn SessionStarter>)
            .map_err(|status| status.code);
        let outcome = starter
            .start_for_operator(a_claude_cli_start())
            .await
            .map_err(|status| status.code);

        // Then
        assert_eq!(
            (rebind, outcome, second.received().len()),
            (
                Err(Code::FailedPrecondition),
                Ok(OperatorStartOutcome::Started {
                    session_id: "from-the-first".to_string()
                }),
                0
            )
        );
    }
}

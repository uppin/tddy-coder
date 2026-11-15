use crate::exec_tool::project_home::DaemonProjectHome;
use async_trait::async_trait;
use tddy_coding_agent_specs::Capability;
use tddy_core::session_lifecycle::unified_session_dir_path;
use tddy_discovery::subagent::PermissionAnswer;
use tddy_session_activity::answers::{
    answer_permission, answer_question, AnswerContext, AnsweringCaller, QuestionAnswer,
};
use tddy_tool_engine::project_home_answer_tools::ProjectHomeAnswerPort;
use tddy_worktree_service::session_target::{HomeCaller, SessionTarget};

#[async_trait]
impl ProjectHomeAnswerPort for DaemonProjectHome {
    /// Deliver the calling home session's `answer` to the hook `target` holds open on this host.
    /// Who may answer is the target's changeset `orchestrator_session_id`, on the host it records:
    /// the caller's own host id is the one that confirmed it (`#home-inbox` 6/8).
    async fn answer_permission(
        &self,
        home: &HomeCaller,
        target: &SessionTarget,
        answer: PermissionAnswer,
        message: Option<String>,
    ) -> Result<serde_json::Value, String> {
        let answered = answer_permission(
            &AnswerContext {
                sessions_base: &self.sessions_base,
                this_daemon_instance_id: &self.daemon_instance_id,
                pending: &self.pending_answers,
            },
            &AnsweringCaller {
                session_id: home.session_id.clone(),
                daemon_instance_id: self.caller_daemon_instance_id.clone(),
            },
            &target.session_id,
            answer,
            message,
        )
        .map_err(|refusal| refusal.to_string())?;
        Ok(serde_json::json!({
            "session_id": answered.session_id,
            "request_id": answered.request_id,
            "answered_by": answered.answered_by,
            "option_id": answered.option_id,
            "cancelled": answered.cancelled,
            "at_unix_ms": answered.at_unix_ms,
        }))
    }

    /// Deliver the calling home session's `answers` to the `AskUserQuestion` `target`'s hook waits
    /// on under `request_id`, from this host's registry (`#home-inbox` 8/9). Who may answer and what
    /// fits the question are checked there. A target whose agent declares no `ask_user_question`
    /// (Muse: its `PreToolUse` never fires for `request_user_input`, so no hook of ours can carry
    /// an answer back) is refused first, naming the terminal tools that can answer it instead.
    async fn answer_question(
        &self,
        home: &HomeCaller,
        target: &SessionTarget,
        request_id: &str,
        answers: Vec<QuestionAnswer>,
    ) -> Result<serde_json::Value, String> {
        self.refuse_a_target_whose_agent_takes_no_structured_answer(target)?;
        let answered = answer_question(
            &AnswerContext {
                sessions_base: &self.sessions_base,
                this_daemon_instance_id: &self.daemon_instance_id,
                pending: &self.pending_answers,
            },
            &AnsweringCaller {
                session_id: home.session_id.clone(),
                daemon_instance_id: self.caller_daemon_instance_id.clone(),
            },
            &target.session_id,
            request_id,
            answers,
        )
        .map_err(|refusal| refusal.to_string())?;
        let answers: Vec<serde_json::Value> = answered
            .answers
            .iter()
            .map(|answer| {
                serde_json::json!({
                    "question_index": answer.question_index,
                    "selected": answer.selected,
                    "other": answer.other,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "session_id": answered.session_id,
            "request_id": answered.request_id,
            "answered_by": answered.answered_by,
            "answers": answers,
            "at_unix_ms": answered.at_unix_ms,
        }))
    }
}

impl DaemonProjectHome {
    /// Refuse a target whose agent cannot take a structured answer: one a coding agent declares
    /// but without `ask_user_question`. Sessions no agent declares (a `tool` session's
    /// clarification) fall through to the checks below, which speak for them.
    fn refuse_a_target_whose_agent_takes_no_structured_answer(
        &self,
        target: &SessionTarget,
    ) -> Result<(), String> {
        let session_dir = unified_session_dir_path(&self.sessions_base, &target.session_id);
        let metadata = tddy_core::read_session_metadata(&session_dir)
            .map_err(|e| format!("cannot read session {}'s record: {e}", target.session_id))?;
        let session_type = metadata.session_type.as_deref().unwrap_or_default();
        let Some(agent) = self
            .launch
            .coding_agents
            .agents()
            .for_session_type(session_type)
        else {
            return Ok(());
        };
        if agent.has(Capability::AskUserQuestion) {
            return Ok(());
        }
        Err(format!(
            "session {} runs {}, which takes no structured answer: drive its terminal with ClaimSessionTerminal and SendSessionKeys instead",
            target.session_id,
            agent.product()
        ))
    }
}

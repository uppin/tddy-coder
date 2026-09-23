//! The questions awaiting an operator — clarifications, permission prompts and the session
//! document under review — and the answers collected for them.

use super::Presenter;
use crate::backend::QuestionOption;
use crate::presenter::presenter_events::PresenterEvent;
use crate::presenter::state::{ActivityKind, AppMode};
use crate::presenter::state_groups::PendingToolCallResponse;
use crate::toolcall::{ToolCallRequest, ToolCallResponse};
use crate::ClarificationQuestion;

impl Presenter {
    fn prd_body_for_plan_review(&self, content_fallback: &str) -> String {
        self.workflow
            .session_dir
            .as_ref()
            .and_then(|d| self.backend.recipe.read_primary_session_document_utf8(d))
            .unwrap_or_else(|| content_fallback.to_string())
    }

    /// Send workflow answer `Approve`, switch to [`AppMode::Running`], and broadcast (shared by DocumentReview and MarkdownViewer).
    fn approve_plan_from_review_or_viewer(&mut self) {
        if let Some(ref tx) = self.workflow.answer_tx {
            let _ = tx.send("Approve".to_string());
        }
        self.state.mode = AppMode::Running;
        self.broadcast_mode_changed();
    }

    fn clarification_answers_ready(&self) -> bool {
        !self.questions.questions.is_empty()
            && self.questions.current_index >= self.questions.questions.len()
            && matches!(self.state.mode, AppMode::Running)
    }

    fn send_clarification_answers(&mut self) {
        let answers = self.collect_answers();
        let is_approve = matches!(
            self.views.pending_tool_call_response,
            Some(PendingToolCallResponse::Approve(_))
        );
        let is_tool_call = self.views.pending_tool_call_response.is_some();
        if let Some(pending) = self.views.pending_tool_call_response.take() {
            match pending {
                PendingToolCallResponse::Ask(tx) => {
                    let _ = tx.send(ToolCallResponse::AskAnswer {
                        answers: answers.clone(),
                    });
                    // `tddy-tools ask` does not go through WaitForInput; merge answers into workflow
                    // context via grill hooks reading this file in `after_task("grill")`.
                    if let Some(ref dir) = self.workflow.session_dir {
                        let wf = dir.join(".workflow");
                        if let Err(e) = std::fs::create_dir_all(&wf) {
                            log::warn!("grill ask answers: create_dir_all {}: {}", wf.display(), e);
                        } else {
                            let path = wf.join("grill_ask_answers.txt");
                            if let Err(e) = crate::atomic_file::write_atomic(&path, &answers) {
                                log::warn!("grill ask answers: write {}: {}", path.display(), e);
                            } else {
                                log::debug!(
                                    "grill ask answers: wrote {} bytes to {}",
                                    answers.len(),
                                    path.display()
                                );
                            }
                        }
                    }
                }
                PendingToolCallResponse::Approve(tx) => {
                    let allow = self
                        .questions
                        .collected_answers
                        .first()
                        .map(|a| a.eq_ignore_ascii_case("Allow"))
                        .unwrap_or(false);
                    let _ = tx.send(ToolCallResponse::ApproveResult { allow });
                }
            }
        } else if let Some(ref answer_tx) = self.workflow.answer_tx {
            let _ = answer_tx.send(answers.clone());
        }
        if is_tool_call {
            let preview: String = answers.chars().take(80).collect();
            let suffix = if answers.len() > 80 { "…" } else { "" };
            let msg = if is_approve {
                format!("✓ permission: {}{}", preview, suffix)
            } else {
                format!("✓ ask answered: {}{}", preview, suffix)
            };
            self.log_activity(msg, ActivityKind::ToolUse);
        }
    }

    fn collect_answers(&self) -> String {
        self.questions.collected_answers.join("\n")
    }

    /// Poll for tool call requests (tddy-tools relay). Call from main loop.
    pub fn poll_tool_calls(&mut self) {
        let rx = match self.views.tool_call_rx.as_ref() {
            Some(r) => r,
            None => return,
        };
        let mut requests = Vec::new();
        while let Ok(req) = rx.try_recv() {
            requests.push(req);
        }
        for req in requests {
            match req {
                ToolCallRequest::SubmitActivity { goal, .. } => {
                    self.log_activity(
                        format!("⚙ tddy-tools submit (goal: {})", goal),
                        ActivityKind::ToolUse,
                    );
                    self.log_activity(
                        format!("✓ submit accepted (goal: {})", goal),
                        ActivityKind::ToolUse,
                    );
                }
                ToolCallRequest::Ask {
                    questions,
                    response_tx,
                } => {
                    let summary: Vec<String> = questions
                        .iter()
                        .map(|q| {
                            let truncated: String = q.question.chars().take(60).collect();
                            if q.question.len() > 60 {
                                format!("{}…", truncated)
                            } else {
                                truncated
                            }
                        })
                        .collect();
                    self.log_activity(
                        format!(
                            "⚙ tddy-tools ask ({} question{}): {}",
                            questions.len(),
                            if questions.len() == 1 { "" } else { "s" },
                            summary.join(" | ")
                        ),
                        ActivityKind::ToolUse,
                    );
                    self.log_activity(
                        "Answer in the TUI question strip at the top (↑/↓ Enter). Not in Cursor."
                            .to_string(),
                        ActivityKind::ToolUse,
                    );
                    self.flush_agent_output_buffer();
                    self.questions.questions = questions;
                    self.questions.current_index = 0;
                    self.questions.collected_answers.clear();
                    self.views.pending_tool_call_response =
                        Some(PendingToolCallResponse::Ask(response_tx));
                    self.advance_to_next_question();
                }
                ToolCallRequest::Approve {
                    tool_name,
                    input,
                    response_tx,
                } => {
                    let detail = match input.get("command").and_then(|c| c.as_str()) {
                        Some(cmd) => {
                            if cmd.len() > 80 {
                                format!("{}…", &cmd[..80])
                            } else {
                                cmd.to_string()
                            }
                        }
                        None => {
                            let s = input.to_string();
                            if s.len() > 80 {
                                format!("{}…", &s[..80])
                            } else {
                                s
                            }
                        }
                    };
                    self.log_activity(
                        format!("⚙ Permission request: {} — {}", tool_name, detail),
                        ActivityKind::ToolUse,
                    );
                    self.flush_agent_output_buffer();
                    let question = ClarificationQuestion {
                        header: "Permission".to_string(),
                        question: format!("Allow {}?", detail),
                        options: vec![
                            QuestionOption {
                                label: "Allow".to_string(),
                                description: "Allow this tool".to_string(),
                            },
                            QuestionOption {
                                label: "Deny".to_string(),
                                description: "Deny this tool".to_string(),
                            },
                        ],
                        multi_select: false,
                        allow_other: false,
                    };
                    self.questions.questions = vec![question];
                    self.questions.current_index = 0;
                    self.questions.collected_answers.clear();
                    self.views.pending_tool_call_response =
                        Some(PendingToolCallResponse::Approve(response_tx));
                    self.advance_to_next_question();
                }
            }
        }
    }

    /// Approve the session document under review.
    pub(super) fn approve_session_document(&mut self) {
        self.state.plan_refinement_pending = false;
        log::info!("ApproveSessionDocument: mode={:?}", self.state.mode);
        if matches!(
            self.state.mode,
            AppMode::DocumentReview { .. } | AppMode::MarkdownViewer { .. }
        ) {
            self.approve_plan_from_review_or_viewer();
        }
    }

    /// Open the session document under review in the viewer.
    pub(super) fn view_session_document(&mut self) {
        if let AppMode::DocumentReview { ref content } = self.state.mode {
            let viewer_content = self.prd_body_for_plan_review(content);
            self.state.plan_refinement_pending = false;
            self.state.mode = AppMode::MarkdownViewer {
                content: viewer_content,
            };
            self.broadcast_mode_changed();
        }
    }

    /// Reject the session document under review.
    pub(super) fn reject_session_document(&mut self) {
        if matches!(
            self.state.mode,
            AppMode::DocumentReview { .. } | AppMode::MarkdownViewer { .. }
        ) {
            self.state.plan_refinement_pending = false;
            if let Some(ref tx) = self.workflow.answer_tx {
                let _ = tx.send("reject".to_string());
            }
        }
    }

    /// Ask for refinement feedback on the session document.
    pub(super) fn refine_session_document(&mut self) {
        log::info!("RefineSessionDocument: mode={:?}", self.state.mode);
        self.state.plan_refinement_pending = true;
        match self.state.mode.clone() {
            AppMode::MarkdownViewer { .. } => {
                log::debug!(
                    "RefineSessionDocument: keep MarkdownViewer; refinement via prompt bar"
                );
                self.broadcast_mode_changed();
            }
            AppMode::DocumentReview { content } => {
                let viewer_content = self.prd_body_for_plan_review(&content);
                self.state.mode = AppMode::MarkdownViewer {
                    content: viewer_content,
                };
                log::debug!("RefineSessionDocument: opened MarkdownViewer from DocumentReview");
                self.broadcast_mode_changed();
            }
            _ => {
                log::warn!(
                    "RefineSessionDocument: unexpected mode {:?}; using TextInput fallback",
                    self.state.mode
                );
                self.state.mode = AppMode::TextInput {
                    prompt: "Enter refinement feedback:".to_string(),
                };
                self.broadcast_mode_changed();
            }
        }
    }

    /// Leave the viewer and return to the document review.
    pub(super) fn dismiss_viewer(&mut self) {
        self.state.plan_refinement_pending = false;
        if let AppMode::MarkdownViewer { ref content } = self.state.mode {
            self.state.mode = AppMode::DocumentReview {
                content: content.clone(),
            };
            self.broadcast_mode_changed();
        }
    }

    /// Answer the current clarification question with the option at `idx`.
    pub(super) fn answer_selected_option(&mut self, idx: usize) {
        if let Some(q) = self.questions.questions.get(self.questions.current_index) {
            if idx < q.options.len() {
                let answer = q.options[idx].label.clone();
                self.questions.collected_answers.push(answer);
                self.questions.current_index += 1;
                self.advance_to_next_question();
                if self.clarification_answers_ready() {
                    self.send_clarification_answers();
                }
            }
        }
    }

    /// Answer the current question with free text.
    pub(super) fn answer_other(&mut self, text: String) {
        self.questions.collected_answers.push(text);
        self.questions.current_index += 1;
        self.advance_to_next_question();
        if self.clarification_answers_ready() {
            self.send_clarification_answers();
        }
    }

    /// Answer a multi-select question.
    pub(super) fn answer_multi_select(&mut self, indices: Vec<usize>, other: Option<String>) {
        if let Some(q) = self.questions.questions.get(self.questions.current_index) {
            if q.multi_select
                && !q.allow_other
                && indices.is_empty()
                && other.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true)
            {
                log::debug!("AnswerMultiSelect: rejected empty submission (allow_other=false)");
                return;
            }
            let mut parts: Vec<String> = indices
                .iter()
                .filter_map(|&i| q.options.get(i).map(|o| o.label.clone()))
                .collect();
            if let Some(o) = other {
                parts.push(o);
            }
            self.questions.collected_answers.push(parts.join(", "));
            self.questions.current_index += 1;
            self.advance_to_next_question();
            if self.clarification_answers_ready() {
                self.send_clarification_answers();
            }
        }
    }

    /// Route typed text: plan refinement feedback, or an answer to the current question.
    pub(super) fn answer_text(&mut self, text: String) {
        if self.state.plan_refinement_pending {
            log::info!("AnswerText: plan refinement feedback (len={})", text.len());
            self.state.plan_refinement_pending = false;
            if let Some(ref tx) = self.workflow.answer_tx {
                let _ = tx.send(text);
            }
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
        } else if matches!(self.state.mode, AppMode::MarkdownViewer { .. }) && !text.is_empty() {
            log::info!(
                "AnswerText: plan refinement (direct entry, len={})",
                text.len()
            );
            if let Some(ref tx) = self.workflow.answer_tx {
                let _ = tx.send(text);
            }
            self.state.mode = AppMode::Running;
            self.broadcast_mode_changed();
        } else {
            self.questions.collected_answers.push(text);
            self.questions.current_index += 1;
            self.advance_to_next_question();
            if self.clarification_answers_ready() {
                self.send_clarification_answers();
            }
        }
    }

    /// Start asking the clarification questions the workflow raised.
    pub(super) fn on_clarification_needed(&mut self, questions: Vec<ClarificationQuestion>) {
        self.flush_agent_output_buffer();
        self.questions.awaiting_open_answer = questions.is_empty();
        self.questions.questions = questions;
        self.questions.current_index = 0;
        self.questions.collected_answers.clear();
        self.advance_to_next_question();
    }

    /// Put the session document up for review.
    pub(super) fn on_session_document_approval_needed(&mut self, content: String) {
        self.flush_agent_output_buffer();
        self.state.mode = AppMode::DocumentReview { content };
        self.broadcast_mode_changed();
        // Resync goal/state in case client missed GoalStarted/StateChanged due to broadcast Lagged
        if let Some(ref g) = self.state.current_goal {
            self.broadcast(PresenterEvent::GoalStarted(g.clone()));
        }
        if let Some(ref s) = self.state.current_state {
            self.broadcast(PresenterEvent::StateChanged {
                from: "Planning".to_string(),
                to: s.clone(),
            });
        }
    }
}

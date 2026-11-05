//! What Telegram offers to start, read from the host's coding-agent registry: the CLI agents (a
//! `run:` over a PTY), each agent's models from its spec, and the 64-byte `callback_data` that
//! names a pick by index — an agent id or a model id plus a session uuid would not fit.
//!
//! No agent is named here: an agent dropped into `<tddyhome>/tools/` is offered like a built-in,
//! and a gate reads a capability (`requires_model`, `branch_name_required`), never an id.

use std::fmt;

use tddy_coding_agent_specs::{CodingAgentRegistry, CodingAgentSpec};

/// `/start-agent <prompt>`: start any CLI agent the host offers — the agent keyboard comes first.
pub const START_AGENT_CMD: &str = "/start-agent";

/// Pick the agent of a pending start: `tsa:<agent_idx>|s:<session_id>`, `agent_idx` into
/// [`TelegramAgentChoices::choices`].
pub const CB_TELEGRAM_START_AGENT: &str = "tsa:";

/// Pick the model of a pending CLI start: `tcm:<model_idx>|p:<proj_idx>|s:<session_id>`,
/// `model_idx` into the chosen agent's [`telegram_model_choices`].
pub const CB_TELEGRAM_MODEL: &str = "tcm:";

/// One CLI agent Telegram offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramAgentChoice {
    /// The agent's id in the registry — also its button label.
    pub agent_id: String,
    /// The session type a start of it asks for: `<id>-cli`.
    pub session_type: String,
}

/// The CLI agents of a host's registry, in the order their buttons are shown (by id).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramAgentChoices {
    choices: Vec<TelegramAgentChoice>,
}

impl TelegramAgentChoices {
    /// Every agent of `registry` a CLI session can run — those `for_session_type("<id>-cli")`
    /// answers, so an ACP-only agent is not offered.
    pub fn of(registry: &CodingAgentRegistry) -> Self {
        // TODO(telegram-spec-launch): implement
        let _ = registry;
        todo!("telegram-spec-launch: TelegramAgentChoices::of")
    }

    /// The offered agents, in button order.
    pub fn choices(&self) -> &[TelegramAgentChoice] {
        &self.choices
    }

    /// The agent at `agent_idx`, refused when the index names none.
    pub fn at(&self, agent_idx: usize) -> Result<&TelegramAgentChoice, TelegramPickRefusal> {
        // TODO(telegram-spec-launch): implement
        let _ = agent_idx;
        todo!("telegram-spec-launch: TelegramAgentChoices::at")
    }
}

/// One model button of a CLI agent's model keyboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramModelChoice {
    /// The model id a start passes.
    pub model_id: String,
    /// The button's text. A spec's `models` declares ids only, so this is the id.
    pub label: String,
}

/// The models a start of `spec` may choose from, in button order: `models.default` first, then
/// `models.curated` without it. Empty for an agent that offers none and does not require one —
/// its start skips the model keyboard; refused for an agent that declares `requires_model` and
/// offers none.
pub fn telegram_model_choices(
    spec: &CodingAgentSpec,
) -> Result<Vec<TelegramModelChoice>, TelegramPickRefusal> {
    // TODO(telegram-spec-launch): implement
    let _ = spec;
    todo!("telegram-spec-launch: telegram_model_choices")
}

/// The model at `model_idx` of [`telegram_model_choices`] for `spec`, refused when it names none.
pub fn telegram_model_at(
    spec: &CodingAgentSpec,
    model_idx: usize,
) -> Result<TelegramModelChoice, TelegramPickRefusal> {
    // TODO(telegram-spec-launch): implement
    let _ = (spec, model_idx);
    todo!("telegram-spec-launch: telegram_model_at")
}

/// The branch a Telegram start of `spec` names for a new branch: `feature/<slug(changeset name)>`
/// when the agent declares `branch_name_required`; `None` otherwise, leaving the name to the
/// start, which generates one.
pub fn telegram_new_branch_name(spec: &CodingAgentSpec, changeset_name: &str) -> Option<String> {
    // TODO(telegram-spec-launch): implement
    let _ = (spec, changeset_name);
    todo!("telegram-spec-launch: telegram_new_branch_name")
}

/// `tsa:<agent_idx>|s:<session_id>`.
pub fn telegram_agent_pick_callback(agent_idx: usize, session_id: &str) -> String {
    // TODO(telegram-spec-launch): implement
    let _ = (agent_idx, session_id);
    todo!("telegram-spec-launch: telegram_agent_pick_callback")
}

/// Decode [`telegram_agent_pick_callback`]'s payload. The index is checked against the registry
/// when the pick is handled, not here.
pub fn parse_telegram_agent_pick_callback(callback_data: &str) -> Option<(usize, String)> {
    // TODO(telegram-spec-launch): implement
    let _ = callback_data;
    todo!("telegram-spec-launch: parse_telegram_agent_pick_callback")
}

/// `tcm:<model_idx>|p:<proj_idx>|s:<session_id>`.
pub fn telegram_model_pick_callback(model_idx: usize, proj_idx: usize, session_id: &str) -> String {
    // TODO(telegram-spec-launch): implement
    let _ = (model_idx, proj_idx, session_id);
    todo!("telegram-spec-launch: telegram_model_pick_callback")
}

/// Decode [`telegram_model_pick_callback`]'s payload. The model index is checked against the
/// pending session's agent when the pick is handled, not here.
pub fn parse_telegram_model_pick_callback(callback_data: &str) -> Option<(usize, usize, String)> {
    // TODO(telegram-spec-launch): implement
    let _ = callback_data;
    todo!("telegram-spec-launch: parse_telegram_model_pick_callback")
}

/// Parse `/start-agent <prompt>` from a message body.
pub fn parse_start_agent_prompt(message_text: &str) -> Option<String> {
    // TODO(telegram-spec-launch): implement
    let _ = message_text;
    todo!("telegram-spec-launch: parse_start_agent_prompt")
}

/// Why a Telegram pick could not be honoured. Its text is what the chat is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TelegramPickRefusal {
    /// The host has no coding agents, so there is nothing to start.
    NoCodingAgents { session_type: String },
    /// The session type names no CLI agent of the host's registry.
    UnknownSessionType { session_type: String },
    /// The agent button's index names no offered agent.
    AgentIndexOutOfRange { agent_idx: usize, offered: usize },
    /// The model button's index names no model of the agent.
    ModelIndexOutOfRange {
        agent_id: String,
        model_idx: usize,
        offered: usize,
    },
    /// The agent declares `requires_model` and its spec offers none.
    RequiredModelNotOffered { agent_id: String },
}

impl fmt::Display for TelegramPickRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // TODO(telegram-spec-launch): implement
        let _ = f;
        todo!("telegram-spec-launch: TelegramPickRefusal::fmt")
    }
}

impl std::error::Error for TelegramPickRefusal {}

//! What Telegram offers to start, read from the host's coding-agent registry: the CLI agents (a
//! `run:` over a PTY), each agent's models from its spec, and the 64-byte `callback_data` that
//! names a pick by index — an agent id or a model id plus a session uuid would not fit.
//!
//! No agent is named here: an agent dropped into `<tddyhome>/tools/` is offered like a built-in,
//! and a gate reads a capability (`requires_model`, `branch_name_required`), never an id.

use std::fmt;

use tddy_coding_agent_specs::{
    Capability, CodingAgentRegistry, CodingAgentSpec, CLI_SESSION_TYPE_SUFFIX,
};

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
        let choices = registry
            .all()
            .into_iter()
            .map(|spec| TelegramAgentChoice {
                agent_id: spec.id.clone(),
                session_type: format!("{}{CLI_SESSION_TYPE_SUFFIX}", spec.id),
            })
            .filter(|choice| registry.for_session_type(&choice.session_type).is_some())
            .collect();
        Self { choices }
    }

    /// The offered agents, in button order.
    pub fn choices(&self) -> &[TelegramAgentChoice] {
        &self.choices
    }

    /// The agent at `agent_idx`, refused when the index names none.
    pub fn at(&self, agent_idx: usize) -> Result<&TelegramAgentChoice, TelegramPickRefusal> {
        self.choices
            .get(agent_idx)
            .ok_or(TelegramPickRefusal::AgentIndexOutOfRange {
                agent_idx,
                offered: self.choices.len(),
            })
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
    let models = &spec.agent.models;
    let default = Some(models.default.as_str()).filter(|id| !id.is_empty());
    let ids: Vec<&str> = default
        .into_iter()
        .chain(
            models
                .curated
                .iter()
                .map(String::as_str)
                .filter(|id| Some(*id) != default),
        )
        .collect();
    if ids.is_empty() && spec.has(Capability::RequiresModel) {
        return Err(TelegramPickRefusal::RequiredModelNotOffered {
            agent_id: spec.id.clone(),
        });
    }
    Ok(ids
        .into_iter()
        .map(|id| TelegramModelChoice {
            model_id: id.to_string(),
            label: id.to_string(),
        })
        .collect())
}

/// The model at `model_idx` of [`telegram_model_choices`] for `spec`, refused when it names none.
pub fn telegram_model_at(
    spec: &CodingAgentSpec,
    model_idx: usize,
) -> Result<TelegramModelChoice, TelegramPickRefusal> {
    let choices = telegram_model_choices(spec)?;
    let offered = choices.len();
    choices
        .into_iter()
        .nth(model_idx)
        .ok_or_else(|| TelegramPickRefusal::ModelIndexOutOfRange {
            agent_id: spec.id.clone(),
            model_idx,
            offered,
        })
}

/// The branch a Telegram start of `spec` names for a new branch: `feature/<slug(changeset name)>`
/// when the agent declares `branch_name_required`; `None` otherwise, leaving the name to the
/// start, which generates one.
pub fn telegram_new_branch_name(spec: &CodingAgentSpec, changeset_name: &str) -> Option<String> {
    if !spec.has(Capability::BranchNameRequired) {
        return None;
    }
    let slug = changeset_name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    (!slug.is_empty()).then(|| format!("feature/{slug}"))
}

/// `tsa:<agent_idx>|s:<session_id>`.
pub fn telegram_agent_pick_callback(agent_idx: usize, session_id: &str) -> String {
    format!("{CB_TELEGRAM_START_AGENT}{agent_idx}|s:{session_id}")
}

/// Decode [`telegram_agent_pick_callback`]'s payload. The index is checked against the registry
/// when the pick is handled, not here.
pub fn parse_telegram_agent_pick_callback(callback_data: &str) -> Option<(usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_START_AGENT)?;
    let (agent_part, session_part) = rest.split_once("|s:")?;
    let session_id = session_part.trim();
    if session_id.is_empty() {
        return None;
    }
    Some((agent_part.parse().ok()?, session_id.to_string()))
}

/// `tcm:<model_idx>|p:<proj_idx>|s:<session_id>`.
pub fn telegram_model_pick_callback(model_idx: usize, proj_idx: usize, session_id: &str) -> String {
    format!("{CB_TELEGRAM_MODEL}{model_idx}|p:{proj_idx}|s:{session_id}")
}

/// Decode [`telegram_model_pick_callback`]'s payload. The model index is checked against the
/// pending session's agent when the pick is handled, not here.
pub fn parse_telegram_model_pick_callback(callback_data: &str) -> Option<(usize, usize, String)> {
    let rest = callback_data.strip_prefix(CB_TELEGRAM_MODEL)?;
    let (model_part, after_p) = rest.split_once("|p:")?;
    let (proj_part, session_part) = after_p.split_once("|s:")?;
    let session_id = session_part.trim();
    if session_id.is_empty() {
        return None;
    }
    Some((
        model_part.parse().ok()?,
        proj_part.parse().ok()?,
        session_id.to_string(),
    ))
}

/// Parse `/start-agent <prompt>` from a message body.
pub fn parse_start_agent_prompt(message_text: &str) -> Option<String> {
    let rest = message_text.trim().strip_prefix(START_AGENT_CMD)?;
    Some(rest.trim().to_string())
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
        match self {
            Self::NoCodingAgents { session_type } => write!(
                f,
                "this host has no coding agents installed: Telegram cannot start a {session_type} session"
            ),
            Self::UnknownSessionType { session_type } => write!(
                f,
                "this host's coding agents have no CLI agent for the session type {session_type:?}"
            ),
            Self::AgentIndexOutOfRange { agent_idx, offered } => write!(
                f,
                "agent button {agent_idx} is not offered: this host offers {offered} CLI agents"
            ),
            Self::ModelIndexOutOfRange {
                agent_id,
                model_idx,
                offered,
            } => write!(
                f,
                "model button {model_idx} is not offered for {agent_id}: its tool definition offers {offered} models"
            ),
            Self::RequiredModelNotOffered { agent_id } => write!(
                f,
                "{agent_id} requires a model, and its tool definition offers none"
            ),
        }
    }
}

impl std::error::Error for TelegramPickRefusal {}

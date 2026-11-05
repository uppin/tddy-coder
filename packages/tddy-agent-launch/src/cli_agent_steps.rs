//! The steps the generic CLI spec launch (`opencode_cli_spawn::start_cli_harness_session` and its
//! resume) gains so that an agent with Claude Code's or Cursor's needs launches through it
//! (`#agent-tools 14/19`). Each step is keyed on a spec slot or capability, never on an agent id:
//!
//! - [`HostLaunchSlots`] / [`render_host_launch_slots`]: the `hooks` slot when the spec declares
//!   `hooks`, the MCP and tool-list slots from the role's [`AgentToolset`], the appended system
//!   prompt (a managed workflow's recipe prompt, a main's prompt) and `{home}`.
//! - [`write_slot_files_on_target`]: an ssh session's slot files written on its target
//!   (`slot_file_over_ssh`, bounded by `SLOT_FILE_OVER_SSH_DEADLINE`), never at that path here.
//! - [`start_conversation`] / [`resumed_conversation`]: `coding_agent.conversation` — the daemon's
//!   session id, a minted id (bounded by the slot's mint deadline), the worktree, or a resume token.
//! - [`refuse_a_role_the_agent_cannot_hold`]: a project-home main only for a spec declaring
//!   `project_home_main`; its pre-approved tools are `project_home_launch::spec_role_launch`'s.
//!
//! Feature docs: `docs/ft/daemon/claude-cli-session.md`, `docs/ft/daemon/cursor-cli-session.md`.
//!
//! The managed-workflow, operator-notify and participant-metadata steps reuse the helpers the
//! bespoke claude path calls today (`claude_cli_spawn_steps::managed_claude_cli_launch`,
//! `notify_env::agent_notify_unless_role`, `stack_parent::link_spawned_branch_without_failing_the_spawn`);
//! green moves them behind `Capability::ManagedWorkflow` / `OperatorNotify` on the generic start.

use std::path::{Path, PathBuf};

use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_coding_agent_specs::CodingAgentSpec;
use tddy_core::adapters::ConversationMintError;
use tddy_core::{HookCommandParams, SessionMetadata};
use tddy_rpc::Status;

use crate::harness_from_spec::HarnessLaunch;
use crate::spec_launch::{AgentToolset, HostAgent};

/// What a generic host launch fills beyond its spec's own argv.
#[derive(Default)]
pub(crate) struct HostLaunchSlots<'a> {
    /// The tool server registered and the tools pre-approved and withdrawn.
    pub toolset: AgentToolset,
    /// The session its `hooks` slot writes settings for; `None` writes none (a spec without
    /// `hooks`, or a resume, which keeps the file its start wrote).
    pub hooks: Option<HookCommandParams<'a>>,
    /// Passed with the spec's `instructions.append_system_prompt` flag.
    pub system_prompt_file: Option<&'a Path>,
    /// The ssh target the worktree lives on; `None` for a checkout on this host.
    pub remote_host: Option<&'a str>,
    /// `{home}` of a placed MCP path: the host's `$HOME`.
    pub home: Option<PathBuf>,
}

/// `agent`'s slots for a host launch in `worktree`, its registration files under `session_dir`.
pub(crate) fn render_host_launch_slots(
    agent: &HostAgent,
    worktree: &Path,
    session_dir: &Path,
    slots: HostLaunchSlots<'_>,
) -> Result<RenderedSlots, Status> {
    // TODO(cli-spec-features): implement — `slots::render` with every field of `slots`
    // (`opencode_cli_spawn::render_host_slots` passes them all empty today).
    let _ = (agent, worktree, session_dir, slots);
    todo!("cli-spec-features: render_host_launch_slots")
}

/// Write `slots.files` on `remote_host` over ssh and take them out of `slots`, so the launch's
/// local write (`spec_launch::plan_host_pty`) writes nothing at those paths on this host. A local
/// checkout (`None`) leaves `slots` alone. Refused `DEADLINE_EXCEEDED` naming the host when the
/// target never answers.
pub(crate) fn write_slot_files_on_target(
    slots: &mut RenderedSlots,
    remote_host: Option<&str>,
) -> Result<(), Status> {
    // TODO(cli-spec-features): implement — `slot_file_over_ssh::write_slot_file_over_ssh` per
    // file; `SlotFileOverSshTimedOut` → `Status::deadline_exceeded`.
    let _ = (slots, remote_host);
    todo!("cli-spec-features: write_slot_files_on_target")
}

/// The conversation a launch binds, from the spec's `coding_agent.conversation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LaunchConversation {
    /// The daemon's session id (`session_id`).
    SessionId(String),
    /// A minted id (`chat_id`), recorded on the session (`cursor_chat_id`).
    Minted(String),
    /// The worktree's own (`--continue`); nothing to bind.
    Worktree,
    /// The token the spec's resume adapter read off the last run, when one was recorded.
    ResumeToken(Option<String>),
}

impl LaunchConversation {
    /// `launch` with this conversation's template parameter set.
    pub(crate) fn bound<'a>(&'a self, launch: HarnessLaunch<'a>) -> HarnessLaunch<'a> {
        // TODO(cli-spec-features): implement — SessionId → `session_id`, Minted → `chat_id`,
        // ResumeToken → `resume_token`, Worktree → unchanged.
        let _ = launch;
        todo!("cli-spec-features: LaunchConversation::bound")
    }

    /// The id to record on the session's `.session.yaml` (`cursor_chat_id`): the minted one only.
    pub(crate) fn recorded_chat_id(&self) -> Option<&str> {
        // TODO(cli-spec-features): implement
        todo!("cli-spec-features: LaunchConversation::recorded_chat_id")
    }
}

/// What a first start's conversation is settled from.
pub(crate) struct ConversationStart<'a> {
    pub spec: &'a CodingAgentSpec,
    /// The agent's resolved binary, which a `minted` adapter runs.
    pub binary: &'a Path,
    pub worktree: &'a Path,
    /// The environment the mint runs with on top of the daemon's (the session's git identity).
    pub env: &'a [(String, String)],
    pub session_id: &'a str,
}

/// The conversation a first start binds: the session id, a freshly minted id (bounded by the
/// slot's mint deadline, refused through [`conversation_mint_refused`]), or nothing to bind.
pub(crate) async fn start_conversation(
    start: ConversationStart<'_>,
) -> Result<LaunchConversation, Status> {
    // TODO(cli-spec-features): implement — `spec.conversation()`; `Minted` runs
    // `LaunchAdapters::builtin().conversation_id(adapter)` within `mint_deadline()`.
    let _ = start;
    todo!("cli-spec-features: start_conversation")
}

/// The conversation a resume of `meta` binds: the session id, the recorded minted id (minted now
/// and recorded when none was, as `resume::chat_to_resume` adopts one today), the worktree, or the
/// recorded resume token.
pub(crate) fn resumed_conversation(
    spec: &CodingAgentSpec,
    meta: &SessionMetadata,
) -> Result<LaunchConversation, Status> {
    // TODO(cli-spec-features): implement
    let _ = (spec, meta);
    todo!("cli-spec-features: resumed_conversation")
}

/// The refusal of a start whose conversation `adapter` did not mint an id for `agent_id`:
/// `DEADLINE_EXCEEDED` for a mint past its deadline, `FAILED_PRECONDITION` otherwise, naming the
/// agent and the adapter.
pub(crate) fn conversation_mint_refused(
    agent_id: &str,
    adapter: &str,
    error: &ConversationMintError,
) -> Status {
    // TODO(cli-spec-features): implement —
    // "{agent_id} could not mint its conversation with {adapter}: {error}"
    let _ = (agent_id, adapter, error);
    todo!("cli-spec-features: conversation_mint_refused")
}

/// A starting session's standing role, refused for an agent that cannot hold it: a project-home
/// main (`PROJECT_HOME_ROLE`) of a spec that does not declare `project_home_main` is refused
/// `FAILED_PRECONDITION` naming the agent. An ordinary session (an empty role) is admitted.
pub(crate) fn refuse_a_role_the_agent_cannot_hold(
    spec: &CodingAgentSpec,
    session_role: &str,
) -> Result<(), Status> {
    // TODO(cli-spec-features): implement —
    // "{spec.id} cannot run a project's main: its spec declares no project_home_main"
    let _ = (spec, session_role);
    todo!("cli-spec-features: refuse_a_role_the_agent_cannot_hold")
}

#[cfg(test)]
mod cli_agent_steps_tests;

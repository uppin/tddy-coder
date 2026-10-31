//! The shared jail launcher: a coding agent declared as a tool is run in a sandbox from its
//! definition alone.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! [`LaunchSessions::launch_jailed`] takes the agent's resolved closure ([`JailSpec`]), renders its
//! argv and slots for jail paths, lowers the closure's `home`, `files`, `secrets`, `policy`,
//! `network` and tools into one plan, writes the runner's [`ExecPlan`] to
//! `<session>/sandbox/`[`EXEC_PLAN_FILE`] and spawns `tddy-sandbox-runner --exec-plan` — then waits
//! for ready, bridges and registers the session as the hand-built launches did. Nothing in this
//! path names an agent: the sandboxed `claude-cli` start and relaunch are its first callers, and
//! `#agent-tools 7/12` and `8/12` move the other jailed CLIs onto it.
//!
//! [`ExecPlan`]: tddy_sandbox_runner::ExecPlan

use std::collections::BTreeMap;

use tddy_actions::tool::{TemplateParams, ToolDef};
use tddy_coding_agent_specs::slots::RenderedSlots;
use tddy_core::provider_route::HarnessRoute;
use tddy_rpc::Status;

use crate::launch_ports::LaunchSessions;
use crate::spec_launch::HostAgent;
use crate::svc_start_sandboxed_claude_cli_session::{JailDirs, JailSession};

/// The runner's plan, written under the session's sandbox root (`<session>/sandbox/exec-plan.json`).
pub const EXEC_PLAN_FILE: &str = "exec-plan.json";

/// The agent a jail runs: its resolution on the host and the definitions of its closure.
pub struct JailSpec<'a> {
    pub agent: &'a HostAgent,
    /// The definition of the agent's own tool and of each of its deps — what `home`, `files`,
    /// `secrets`, `policy` and `network` are lowered from.
    pub defs: &'a [ToolDef],
}

/// The session a jail is launched for, beyond what [`JailSpec`] declares.
pub struct JailedSession<'a> {
    pub jail: &'a JailSession<'a>,
    pub dirs: &'a JailDirs,
    /// Host-side env the session's relayed commands run with (a managed workflow's `TDDY_SOCKET`).
    pub session_env: Vec<(String, String)>,
    /// Env the daemon adds for the session, on top of the plan's: subagents, LSP, the semantic
    /// index, the renderer choice.
    pub env: BTreeMap<String, String>,
    pub managed: Option<tddy_cli_sessions::session_toolcall::ManagedWorkflow>,
    /// The provider route the agent runs on; `None` for its own vendor. A route's base-URL host
    /// joins the jail's egress policy, and `when: unrouted` items are dropped.
    pub route: Option<HarnessRoute>,
}

impl LaunchSessions {
    /// Launch `spec` in a jail for `session` and return the runner's pid.
    ///
    /// `role` is the session's granted role (`project-home`; empty for none). It is written into
    /// the session's metadata before the spawn, as the host launch does, so a jailed project-home
    /// main is listed with its role and a resume re-applies it.
    pub async fn launch_jailed(
        &self,
        spec: JailSpec<'_>,
        params: &TemplateParams,
        slots: &RenderedSlots,
        session: JailedSession<'_>,
        role: &str,
    ) -> Result<u32, Status> {
        // TODO(claude-jail): implement — lower the closure (`lower_tools`, `lower_network` plus the
        // route host, `lower_home`, `lower_files`, `lower_secrets`, `lower_policy`), render argv
        // with `render_argv`, write the `ExecPlan`, spawn the runner with `--exec-plan`, then the
        // ready / bridge / register steps `launch_jail` runs today.
        let _ = (self, spec, params, slots, session, role);
        todo!("claude-jail: launch_jailed")
    }
}

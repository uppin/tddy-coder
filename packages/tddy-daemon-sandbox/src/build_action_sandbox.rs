//! `tddy-build`'s hermetic-action port, over the action sandbox runtime (`#agent-tools 4/12`).
//!
//! PRD: `docs/ft/build/1-WIP/PRD-2026-10-31-build-tools-are-tool-defs.md`.
//!
//! [`DaemonActionSandbox`] lowers one build action into a process-mode jail plan:
//!
//! - every input the action declares is a read-only mount;
//! - its declared outputs are the only writable paths;
//! - its tool closure is lowered by `tddy_sandbox_recipes::lower_tools` — each binary a literal
//!   executable read, reachable by name from a jail-local `bin/` first on `PATH`;
//! - its network is `tddy_sandbox_recipes::lower_network` of the same closure, so a closure of only
//!   `egress: none` tools has no network at all.
//!
//! The jail is the platform backend's (Seatbelt on macOS, cgroups on Linux) — the same one
//! `sandbox_action::spawn_sandbox_action` runs a sandboxed `ActionSpec` in.

use std::path::PathBuf;

use futures_util::future::BoxFuture;
use tddy_actions::tool::ResolvedTool;
use tddy_actions::ActionSpec;
use tddy_build::executor::ActionOutcome;
use tddy_build::{ActionSandbox, BuildError};

/// Runs `tddy-build` hermetic actions in a real jail.
#[derive(Debug, Clone)]
pub struct DaemonActionSandbox {
    /// Where each action's jail layout (profile, jail `bin/`, logs) is created, one directory per
    /// action.
    work_dir: PathBuf,
}

impl DaemonActionSandbox {
    /// A sandbox laying out each action's jail under `work_dir`.
    pub fn new(work_dir: PathBuf) -> Self {
        Self { work_dir }
    }
}

impl ActionSandbox for DaemonActionSandbox {
    fn run<'a>(
        &'a self,
        spec: &'a ActionSpec,
        tools: &'a [ResolvedTool],
    ) -> BoxFuture<'a, Result<ActionOutcome, BuildError>> {
        // TODO(build-tools): implement — `lower_tools` + `lower_network` of `tools` into a process
        // plan with `spec.inputs` read-only and `spec.outputs` writable, spawned through the
        // platform backend under `self.work_dir/<action id>`; the outcome's stderr carries the
        // jail's refusal.
        let _ = (&self.work_dir, spec, tools);
        todo!("build-tools: DaemonActionSandbox::run")
    }
}

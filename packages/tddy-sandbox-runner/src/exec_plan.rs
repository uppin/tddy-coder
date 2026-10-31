//! `--exec-plan <file>`: the runner's generic tool mode — exec a rendered argv with a declared env
//! and files, knowing nothing of which tool it is.
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-10-31-claude-cli-jail-from-spec.md` (`#agent-tools 5/12`).
//!
//! The daemon's jail launcher (`tddy_agent_launch::jail_launch`) renders a tool's argv, env and
//! slot files from its definition and writes them here as JSON; the runner writes the files, sets
//! the env, and spawns the argv in its PTY. What only the agent modes had — the egress proxy env
//! and `TDDY_SECRET_*` resolution — is applied exactly as they apply it, so the process sees an
//! ordinary environment and no `--claude-*` flag ever reaches the runner.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::runner::SandboxRunnerArgs;

/// One process the runner execs, fully rendered by the host.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecPlan {
    /// The argv, `argv[0]` the binary as the jail reaches it.
    pub argv: Vec<String>,
    /// Set on the process on top of the jail's env. `TDDY_SECRET_<NAME>` entries name secret files
    /// and are resolved, never passed through.
    pub env: BTreeMap<String, String>,
    /// Written before the spawn, each at its jail path.
    pub files: Vec<ExecPlanFile>,
    /// Point the process at the runner's egress shim (`HTTPS_PROXY` and friends, both casings).
    pub egress_shim: bool,
}

/// A file an [`ExecPlan`] writes before its process starts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecPlanFile {
    pub path: PathBuf,
    pub contents: Vec<u8>,
}

impl ExecPlan {
    /// The process's environment: `jail_env` (the runner's own, as the jail set it), then
    /// [`Self::env`] with every `TDDY_SECRET_<NAME>` resolved to `<NAME>` and its file unlinked
    /// (the same resolution `resolve_secret_envs` gives the agent modes), then — when
    /// [`Self::egress_shim`] is set — the proxy env for `shim_port` in both casings, with loopback
    /// bypassed.
    pub fn child_env(
        &self,
        jail_env: &BTreeMap<String, String>,
        shim_port: u16,
    ) -> BTreeMap<String, String> {
        // TODO(claude-jail): implement — reuse `resolve_secret_envs` and `egress_proxy_env`.
        let _ = (jail_env, shim_port);
        todo!("claude-jail: ExecPlan::child_env")
    }
}

/// Run the plan at `plan` under `args`: the same boot, `SessionChannel` server and egress shim as
/// [`crate::run_sandbox_runner`], then write [`ExecPlan::files`] and spawn [`ExecPlan::argv`] in the
/// PTY with [`ExecPlan::child_env`]. No in-jail `tddy-tools --mcp` is started and no agent argv is
/// built: the plan's argv already registers whatever server its tool declares.
pub async fn run_exec_plan(args: SandboxRunnerArgs, plan: PathBuf) -> Result<()> {
    // TODO(claude-jail): implement.
    let _ = (args, plan);
    todo!("claude-jail: run_exec_plan")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    const SHIM_PORT: u16 = 47_123;
    const SHIM: &str = "http://127.0.0.1:47123";

    fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn a_plan(env: &[(&str, &str)], egress_shim: bool) -> ExecPlan {
        ExecPlan {
            argv: vec!["/usr/local/bin/claude".to_string()],
            env: vars(env),
            files: Vec::new(),
            egress_shim,
        }
    }

    #[test]
    fn a_plan_asking_for_the_shim_gets_the_proxy_env_in_both_casings_on_top_of_its_own() {
        // Given
        let jail_env = vars(&[("TMPDIR", "/jail/tmp")]);
        let plan = a_plan(&[("HOME", "/tddy/homes/claude")], true);

        // When
        let env = plan.child_env(&jail_env, SHIM_PORT);

        // Then
        assert_eq!(
            env,
            vars(&[
                ("TMPDIR", "/jail/tmp"),
                ("HOME", "/tddy/homes/claude"),
                ("HTTPS_PROXY", SHIM),
                ("HTTP_PROXY", SHIM),
                ("https_proxy", SHIM),
                ("http_proxy", SHIM),
                ("NO_PROXY", "127.0.0.1,localhost"),
                ("no_proxy", "127.0.0.1,localhost"),
            ])
        );
    }

    #[test]
    fn a_plan_asking_for_no_shim_gets_no_proxy_env_and_keeps_its_own() {
        // Given
        let jail_env = vars(&[("TMPDIR", "/jail/tmp")]);
        let plan = a_plan(&[("HOME", "/tddy/homes/claude")], false);

        // When
        let env = plan.child_env(&jail_env, SHIM_PORT);

        // Then
        assert_eq!(
            env,
            vars(&[("TMPDIR", "/jail/tmp"), ("HOME", "/tddy/homes/claude")])
        );
    }

    #[test]
    fn a_tddy_secret_file_is_resolved_to_its_real_name_and_unlinked() {
        // Given
        let scratch = tempfile::tempdir().unwrap();
        let secret_file = scratch.path().join("secret-CLAUDE_CODE_OAUTH_TOKEN");
        std::fs::write(&secret_file, "the-oauth-token").unwrap();
        let plan = a_plan(
            &[
                (
                    "TDDY_SECRET_CLAUDE_CODE_OAUTH_TOKEN",
                    secret_file.to_str().unwrap(),
                ),
                ("HOME", "/tddy/homes/claude"),
            ],
            false,
        );

        // When
        let env = plan.child_env(&BTreeMap::new(), SHIM_PORT);

        // Then
        assert_eq!(
            env,
            vars(&[
                ("CLAUDE_CODE_OAUTH_TOKEN", "the-oauth-token"),
                ("HOME", "/tddy/homes/claude"),
            ])
        );
        assert!(
            !secret_file.exists(),
            "the secret file is unlinked once read"
        );
    }
}

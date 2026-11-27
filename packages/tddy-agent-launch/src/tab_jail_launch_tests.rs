//! Unit tests: the provider route a sandboxed session's shell-tab jail is lowered on
//! (`#session-liveness` 5/8) — the account the session's start recorded, re-resolved from the owner's
//! vault as a resume re-resolves it, so the tab's egress is the main jail's and no wider.
//!
//! The vault is the real sealed one of `session_provider_route_tests`. The kernel sandbox the jail is
//! then spawned in is not exercised here: `sandbox_shell_tab_jail_linux` covers it.

// Test-only: included via `mod` under `#[cfg(test)]`, so the filesystem-port guard reads
// the whole file as test code and the real-disk fixtures below are the tests' fixtures, not
// production I/O.
#![cfg(test)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ProcessToolHost, ToolHost};
use tddy_coding_agent_specs::load_agent_registry;
use tddy_core::{write_session_provider_account, SessionProviderAccount};
use tddy_daemon_sandbox::tool_jail::route_hosts;
use tddy_rpc::Code;
use tddy_sandbox::HostPattern;

use crate::harness_from_spec::{cli_launcher, resolve_session_agent, CliSessionAgent};
use crate::session_acting_identity::SessionAccountAccess;
use crate::session_provider_route_tests::{adas_meta_account, adas_vaults_holding, ADA};
use crate::spec_launch::HostCodingAgents;
use crate::tab_jail_launch::tab_route;

/// A host on which every tool's binary override names one executable file: the tests here resolve a
/// session agent to read its route, and the host that runs them (CI) has no `claude` installed. The
/// file is real, so the resolution is the one a daemon does; it is never run.
struct EveryBinaryAt(PathBuf);

impl ToolHost for EveryBinaryAt {
    fn env_var(&self, name: &str) -> Option<String> {
        name.ends_with("_BINARY")
            .then(|| self.0.display().to_string())
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        ProcessToolHost.is_executable_file(path)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        ProcessToolHost.canonicalize(path)
    }
}

/// The `claude-cli` session agent of a fixture host that has `claude` and `tddy-tools` installed.
fn the_claude_session_agent() -> CliSessionAgent {
    let bin_dir = tempfile::tempdir().expect("a bin directory");
    let binary = bin_dir.path().join("stub-binary");
    std::fs::write(&binary, "#!/bin/sh\n").expect("a stub binary");
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
        .expect("an executable stub binary");
    let tools_dir = tempfile::tempdir().expect("a tools directory");
    let registry = load_agent_registry(tools_dir.path()).expect("the built-in agents load");
    let agents = Arc::new(
        HostCodingAgents::try_new(registry, Arc::new(EveryBinaryAt(binary)))
            .expect("the built-in agents are usable"),
    );
    let kind = cli_launcher(Some(&agents), "claude-cli")
        .expect("claude-cli is a declared session type")
        .expect("claude-cli runs a coding agent");
    resolve_session_agent(&agents, kind).expect("the claude agent resolves")
}

/// Access to the vault of Ada, the session's owner, by her subject alone: a tab is opened with no
/// session token in hand, only the GitHub user the token resolved to.
fn adas_vault_by_subject(
    accounts: &[tddy_accounts::ProviderAccount],
) -> (tempfile::TempDir, SessionAccountAccess) {
    let (dir, vaults) = adas_vaults_holding(accounts, true);
    (dir, SessionAccountAccess::for_subject(Some(vaults), ADA))
}

/// A session directory whose start recorded Ada's Meta account.
fn a_session_dir_on_the_meta_account() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a session directory");
    write_session_provider_account(
        dir.path(),
        &SessionProviderAccount {
            account_id: "meta-hackathon".to_string(),
            model: "muse-wonder-1.0".to_string(),
        },
    )
    .expect("recorded at start");
    dir
}

#[test]
fn a_tab_jail_of_a_routed_session_is_lowered_on_its_route_host() {
    // Given a session started on Ada's Meta account, whose vault holds it unlocked
    let (_vault, access) = adas_vault_by_subject(&[adas_meta_account()]);
    let session_dir = a_session_dir_on_the_meta_account();

    // When the tab's route is resolved
    let route = tab_route(&access, &the_claude_session_agent(), session_dir.path())
        .expect("the route resolves")
        .expect("a route, not the vendor");

    // Then the tab's egress admits the route's host, as the main jail's does
    assert_eq!(
        route_hosts(&route),
        vec![HostPattern::new("api.meta.ai", 443)]
    );
}

#[test]
fn a_tab_jail_of_a_session_on_its_vendor_has_no_route() {
    // Given a session directory with no account record
    let (_vault, access) = adas_vault_by_subject(&[adas_meta_account()]);
    let session_dir = tempfile::tempdir().expect("a session directory");

    // When / Then the tab is lowered on the agent's own egress
    assert_eq!(
        tab_route(&access, &the_claude_session_agent(), session_dir.path())
            .expect("no record resolves to no route"),
        None
    );
}

#[test]
fn a_tab_jail_whose_sessions_account_was_removed_is_refused_not_run_on_the_vendor() {
    // Given a session recorded on an account its owner's vault no longer holds
    let (_vault, access) = adas_vault_by_subject(&[]);
    let session_dir = a_session_dir_on_the_meta_account();

    // When the tab's route is resolved
    let refusal = tab_route(&access, &the_claude_session_agent(), session_dir.path())
        .expect_err("a removed account refuses the tab, as it refuses the resume");

    // Then
    assert_eq!(refusal.code(), Code::NotFound);
}

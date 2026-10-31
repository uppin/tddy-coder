//! Acceptance: a workspace jail runs its declared tools — `rg` today — as the binary file the
//! registry resolves, reachable by name, and refuses to start when the host cannot satisfy one.
//!
//! PRD: `docs/ft/coder/1-WIP/PRD-2026-10-31-declared-tools.md` (`#agent-tools 1/12`); replaces the
//! `host_ripgrep_grants` patch (`docs/dev/todo/2026-09-26-grep-is-unreachable-inside-every-jail.md`).
//!
//! The two Seatbelt tests run on a developer's Mac only: every CI job is `ubuntu-*`, the same gap
//! `docs/dev/todo/2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md` records. They are the
//! only proof of the defect's fix, because it only exists under the real loader.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::{ResolvedTool, ToolHost};
use tddy_daemon_sandbox::workspace_tool_sandbox::{
    build_workspace_tool_plan, JailedWorkspaceSandboxProvisioner, WorkspaceSandboxLayout,
    WorkspaceSandboxProvisioner, WorkspaceSandboxSpec, WorkspaceToolPlanRequest,
};
use tddy_sandbox::{ReadReason, ReadSpec, SandboxPlan};

/// A host whose environment and `PATH` the test declares, over the real filesystem.
#[derive(Default)]
struct AHost {
    env: BTreeMap<String, String>,
    path: Vec<PathBuf>,
}

fn a_host() -> AHost {
    AHost::default()
}

impl AHost {
    fn with_env(mut self, name: &str, value: &Path) -> Self {
        self.env
            .insert(name.into(), value.to_string_lossy().into_owned());
        self
    }

    fn with_path(mut self, dirs: &[&Path]) -> Self {
        self.path = dirs.iter().map(|d| d.to_path_buf()).collect();
        self
    }
}

impl ToolHost for AHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }
    fn path_dirs(&self) -> Vec<PathBuf> {
        self.path.clone()
    }
    fn is_executable_file(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A session directory with a checkout to search, both canonical (Seatbelt matches resolved paths).
struct ASession {
    session_dir: PathBuf,
    worktree: PathBuf,
    _tmp: tempfile::TempDir,
}

fn a_session_with_a_file_to_find() -> ASession {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = std::fs::canonicalize(tmp.path()).expect("canonical tempdir");
    let session_dir = root.join("sessions").join("declared-tools");
    let worktree = root.join("worktree");
    std::fs::create_dir_all(&session_dir).expect("session dir");
    std::fs::create_dir_all(&worktree).expect("worktree");
    std::fs::write(worktree.join("hay.rs"), "const NEEDLE: u32 = 1;\n").expect("a file");
    ASession {
        session_dir,
        worktree,
        _tmp: tmp,
    }
}

fn the_spec_of(session: &ASession) -> WorkspaceSandboxSpec {
    WorkspaceSandboxSpec {
        session_id: "declared-tools".into(),
        session_dir: session.session_dir.clone(),
        worktree_path: session.worktree.clone(),
    }
}

fn a_plan_for(session: &ASession, tools: Vec<ResolvedTool>) -> SandboxPlan {
    build_workspace_tool_plan(WorkspaceToolPlanRequest {
        layout: WorkspaceSandboxLayout::under_session_dir(&session.session_dir),
        worktree_path: session.worktree.clone(),
        session_id: "declared-tools".into(),
        runner_path: "/usr/bin/true".into(),
        tddy_tools_path: "/usr/bin/true".into(),
        cgroup: Default::default(),
        tools,
    })
    .expect("the plan builds")
}

/// The jail's `PATH`, as the plan declares it.
fn the_jail_path(plan: &SandboxPlan) -> String {
    plan.env.vars.get("PATH").cloned().expect("a jail PATH")
}

#[test]
fn a_host_that_cannot_resolve_rg_refuses_the_workspace_jail_at_start_naming_the_tool_and_the_search(
) {
    // Given — a host with no `rg` on its PATH and no override
    let session = a_session_with_a_file_to_find();
    let empty_bin = tempfile::tempdir().unwrap();
    let tools_dir = tempfile::tempdir().unwrap();
    let provisioner = JailedWorkspaceSandboxProvisioner::new(
        tools_dir.path().to_path_buf(),
        Arc::new(a_host().with_path(&[empty_bin.path()])),
    );

    // When
    let refused = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(provisioner.provision(&the_spec_of(&session)))
        .err()
        .expect("the jail must not start");

    // Then — refused before anything of the jail exists
    assert_eq!(
        refused.to_string(),
        format!(
            "invalid sandbox spec: tool rg is unavailable: searched TDDY_RG_BINARY (unset), PATH ({})",
            empty_bin.path().display()
        )
    );
    assert!(!session.session_dir.join("sandbox").exists());
}

#[test]
fn an_override_in_tddyhome_tools_replaces_the_builtin_rg_and_the_jail_uses_it() {
    // Given — `<tddyhome>/tools/rg.yaml` points `rg` at its own variable, set to a binary here
    let session = a_session_with_a_file_to_find();
    let alt = tempfile::tempdir().unwrap();
    let alt_rg = std::fs::canonicalize(alt.path()).unwrap().join("rg");
    std::fs::copy("/bin/echo", &alt_rg).expect("a stand-in rg");
    let tools_dir = tempfile::tempdir().unwrap();
    std::fs::write(
        tools_dir.path().join("rg.yaml"),
        "schema_version: 1\nid: rg\nsummary: a pinned ripgrep\nbinary:\n  command: rg\n  env: TDDY_PINNED_RG\n",
    )
    .unwrap();
    let provisioner = JailedWorkspaceSandboxProvisioner::new(
        tools_dir.path().to_path_buf(),
        Arc::new(a_host().with_env("TDDY_PINNED_RG", &alt_rg)),
    );

    // When
    let tools = provisioner
        .resolve_jail_tools()
        .expect("the override resolves");
    let plan = a_plan_for(&session, tools);

    // Then — the jail grants exactly that file, executable
    assert_eq!(
        plan.reads
            .iter()
            .filter(|r| r.host == alt_rg)
            .cloned()
            .collect::<Vec<_>>(),
        vec![ReadSpec::literal(&alt_rg, ReadReason::BinaryDeps).executable()]
    );
}

#[cfg(target_os = "macos")]
mod under_seatbelt {
    use super::{
        a_host, a_plan_for, a_session_with_a_file_to_find, the_jail_path,
        JailedWorkspaceSandboxProvisioner,
    };
    use pretty_assertions::assert_eq;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::Arc;
    use tddy_sandbox::SandboxPlan;

    /// The host's own `rg` directory — this suite needs ripgrep installed.
    fn the_hosts_rg_dir() -> PathBuf {
        let out = Command::new("/usr/bin/which").arg("rg").output().unwrap();
        assert!(
            out.status.success(),
            "this test needs ripgrep on PATH — `brew install ripgrep`"
        );
        PathBuf::from(String::from_utf8(out.stdout).unwrap().trim())
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// Render `plan`, materialize its links, and run `argv` inside it with the plan's own `PATH`.
    fn run_in_jail(plan: &SandboxPlan, profile_dir: &Path, argv: &[&str]) -> std::process::Output {
        tddy_sandbox::materialize_symlinks(&plan.symlinks).expect("the jail's links");
        let profile = profile_dir.join("probe.sb");
        std::fs::write(&profile, tddy_sandbox_darwin::render_plan(plan).unwrap()).unwrap();
        Command::new("sandbox-exec")
            .arg("-f")
            .arg(&profile)
            .arg("/usr/bin/env")
            .arg(format!("PATH={}", the_jail_path(plan)))
            .args(argv)
            .output()
            .expect("sandbox-exec runs")
    }

    #[test]
    fn a_workspace_jail_built_from_the_declared_rg_tool_runs_rg_over_its_worktree_under_seatbelt() {
        // Given — the registry's `rg`, resolved against this Mac's PATH
        let session = a_session_with_a_file_to_find();
        let tools_dir = tempfile::tempdir().unwrap();
        let provisioner = JailedWorkspaceSandboxProvisioner::new(
            tools_dir.path().to_path_buf(),
            Arc::new(a_host().with_path(&[&the_hosts_rg_dir(), Path::new("/usr/bin")])),
        );
        let plan = a_plan_for(&session, provisioner.resolve_jail_tools().unwrap());

        // When — `rg` is invoked by name, as `tool_grep` does
        let out = run_in_jail(
            &plan,
            &session.session_dir,
            &[
                "rg",
                "--no-config",
                "NEEDLE",
                session.worktree.to_str().unwrap(),
            ],
        );

        // Then
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim_end(),
            format!(
                "{}:1:const NEEDLE: u32 = 1;",
                session.worktree.join("hay.rs").display()
            ),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn the_jail_grants_the_resolved_rg_file_and_no_sibling_binary_in_its_directory_is_executable() {
        // Given — `rg` and an unrelated binary side by side, as in `/opt/homebrew/bin`
        let session = a_session_with_a_file_to_find();
        let bin = tempfile::tempdir().unwrap();
        let bin_dir = std::fs::canonicalize(bin.path()).unwrap();
        let real_rg = std::fs::canonicalize(the_hosts_rg_dir().join("rg")).unwrap();
        std::fs::copy(&real_rg, bin_dir.join("rg")).unwrap();
        std::fs::copy("/bin/echo", bin_dir.join("sibling")).unwrap();
        let tools_dir = tempfile::tempdir().unwrap();
        let provisioner = JailedWorkspaceSandboxProvisioner::new(
            tools_dir.path().to_path_buf(),
            Arc::new(a_host().with_path(&[&bin_dir])),
        );
        let plan = a_plan_for(&session, provisioner.resolve_jail_tools().unwrap());

        // When
        let sibling = run_in_jail(
            &plan,
            &session.session_dir,
            &[bin_dir.join("sibling").to_str().unwrap(), "escaped"],
        );

        // Then — the jail never runs it
        assert_eq!(String::from_utf8_lossy(&sibling.stdout), "");
        assert!(!sibling.status.success());
    }
}

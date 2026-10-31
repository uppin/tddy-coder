//! Acceptance: a `tddy-build` action with `sandbox: true` runs in a real jail where its inputs are
//! read-only, its declared outputs are the only writable paths, its tool closure is the only set of
//! executables beyond the jail's baseline, and its network is what that closure declares.
//!
//! PRD: `docs/ft/build/1-WIP/PRD-2026-10-31-build-tools-are-tool-defs.md` (`#agent-tools 4/12`).
//!
//! Every test drives `tddy-build`'s executor with [`DaemonActionSandbox`] as its `ActionSandbox`,
//! against a real repository in a tempdir. The claim is confinement, which only the real backend
//! shows: Seatbelt on a developer's Mac, cgroups on Linux. Every CI job is `ubuntu-*`, so the
//! Seatbelt half runs on a developer's Mac only; the Linux half is the one CI runs. Each absence
//! (no write, no exec, no network) is paired with the same action run unconfined, so a test cannot
//! pass because the action was broken to begin with.
#![cfg(any(target_os = "macos", target_os = "linux"))]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_build::executor::{execute_target_with_tools, ActionOutcome, BuildTools, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build::{load_build_manifest, BuildMode};
use tddy_daemon_sandbox::build_action_sandbox::DaemonActionSandbox;

/// A host whose environment the test declares, with only the system directories on `PATH`, over
/// the real filesystem.
#[derive(Default)]
struct AHost {
    env: BTreeMap<String, String>,
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
}

impl ToolHost for AHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }
    fn path_dirs(&self) -> Vec<PathBuf> {
        vec![PathBuf::from("/bin"), PathBuf::from("/usr/bin")]
    }
    fn is_executable_file(&self, path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// A repository with one `BUILD.yaml`, an empty `<tddyhome>/tools`, and a jail work directory —
/// all canonical, because Seatbelt matches resolved paths.
struct ARepo {
    root: PathBuf,
    tools_dir: PathBuf,
    jail_work: PathBuf,
    _tmp: tempfile::TempDir,
}

fn a_repo_with_manifest(yaml: &str) -> ARepo {
    a_repo().with_manifest(yaml)
}

fn a_repo() -> ARepo {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = std::fs::canonicalize(tmp.path()).expect("canonical tempdir");
    let repo = ARepo {
        root: base.join("repo"),
        tools_dir: base.join("tddyhome").join("tools"),
        jail_work: base.join("jails"),
        _tmp: tmp,
    };
    for dir in [&repo.root, &repo.tools_dir, &repo.jail_work] {
        std::fs::create_dir_all(dir).expect("a repo directory");
    }
    repo
}

impl ARepo {
    fn with_manifest(self, yaml: &str) -> Self {
        std::fs::write(self.path("BUILD.yaml"), yaml).expect("BUILD.yaml");
        self
    }

    fn with_file(self, rel: &str, contents: &str) -> Self {
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("file dir");
        std::fs::write(path, contents).expect("a file");
        self
    }

    fn with_script(self, rel: &str, body: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let path = self.path(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("script dir");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("script");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        self
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    }

    /// Build `target` with hermetic actions running in a real jail; the outcome of its one action.
    async fn build(&self, target: &str, host: AHost) -> ActionOutcome {
        let yaml = std::fs::read_to_string(self.path("BUILD.yaml")).expect("BUILD.yaml");
        let graph = BuildGraph::from_manifests(vec![load_build_manifest(&yaml).expect("manifest")])
            .expect("graph");
        let tools = BuildTools::load(&self.tools_dir, Arc::new(host))
            .expect("the tools load")
            .with_sandbox(Arc::new(DaemonActionSandbox::new(self.jail_work.clone())));
        let mut record = execute_target_with_tools(
            &self.root,
            &graph,
            target,
            &ExecuteOptions {
                no_cache: true,
                ..ExecuteOptions::default()
            },
            BuildMode::Compile,
            &PluginRegistry::new(),
            &tools,
        )
        .await
        .expect("the build runs");
        assert_eq!(record.actions.len(), 1, "one action per test target");
        record.actions.remove(0)
    }
}

/// A target `id` with one action running `script` under `sh -c`, hermetic or not.
fn a_target(id: &str, sandbox: bool, script: &str, extra: &str) -> String {
    format!(
        r#"
  - id: "{id}"
    actions:
      - id: run
        type: command
        command: ["sh", "-c", {script:?}]
        sandbox: {sandbox}
{extra}"#
    )
}

fn a_manifest(targets: &[String]) -> String {
    format!("schema_version: 1\ntargets:{}\n", targets.concat())
}

const IN_AND_OUT: &str = r#"        inputs: [{ include: ["in.txt"], root: "." }]
        outputs: [{ path: "out.txt", kind: file }]
"#;

#[tokio::test]
async fn a_hermetic_action_writing_only_its_declared_output_succeeds() {
    // Given
    let repo = a_repo_with_manifest(&a_manifest(&[a_target(
        "copy:jailed",
        true,
        "tr a-z A-Z < in.txt > out.txt",
        IN_AND_OUT,
    )]))
    .with_file("in.txt", "hermetic\n");

    // When
    let outcome = repo.build("copy:jailed", a_host()).await;

    // Then
    assert_eq!((outcome.exit_code, outcome.stderr.as_str()), (0, ""));
    assert_eq!(repo.read("out.txt"), "HERMETIC\n");
}

#[tokio::test]
async fn a_hermetic_action_writing_outside_its_outputs_fails_naming_the_path() {
    // Given — the same script, unconfined and hermetic; it writes its output, then a stray file
    let script = "cat in.txt > out.txt && echo leak > stray.txt";
    let repo = a_repo_with_manifest(&a_manifest(&[
        a_target("leak:open", false, script, IN_AND_OUT),
        a_target("leak:jailed", true, script, IN_AND_OUT),
    ]))
    .with_file("in.txt", "data\n");
    let open = repo.build("leak:open", a_host()).await;
    assert_eq!(
        (open.exit_code, repo.read("stray.txt")),
        (0, "leak\n".to_string())
    );
    std::fs::remove_file(repo.path("stray.txt")).expect("reset the positive control");

    // When
    let jailed = repo.build("leak:jailed", a_host()).await;

    // Then — the declared output was written, the stray write refused
    assert_eq!(jailed.exit_code, 1);
    assert_eq!(repo.read("out.txt"), "data\n");
    assert!(!repo.path("stray.txt").exists());
    // The refusal is the shell's own message, whose wording differs between `sh`s and backends;
    // what the criterion fixes is that it names the path.
    assert!(
        jailed.stderr.contains("stray.txt"),
        "the refusal names the path: {:?}",
        jailed.stderr
    );
}

#[tokio::test]
async fn a_hermetic_action_execing_a_binary_outside_its_tool_closure_fails_naming_it() {
    // Given — an executable in the repository that no tool declares; the same exec, unconfined
    // and hermetic
    let repo = a_repo().with_script("vendor/outsider", "echo OUTSIDER-RAN");
    let outsider = repo.path("vendor/outsider").display().to_string();
    let repo = repo.with_manifest(&a_manifest(&[
        a_target("exec:open", false, &outsider, ""),
        a_target("exec:jailed", true, &outsider, ""),
    ]));
    let open = repo.build("exec:open", a_host()).await;
    assert_eq!(
        (open.exit_code, open.stdout.as_str()),
        (0, "OUTSIDER-RAN\n")
    );

    // When
    let jailed = repo.build("exec:jailed", a_host()).await;

    // Then — `sh` could not execute it
    assert_eq!((jailed.exit_code, jailed.stdout.as_str()), (126, ""));
    // The shell's wording differs between `sh`s and backends; the criterion is that it names the
    // binary.
    assert!(
        jailed.stderr.contains(&outsider),
        "the refusal names the binary: {:?}",
        jailed.stderr
    );
}

#[tokio::test]
async fn a_hermetic_action_with_no_network_declaring_tool_has_no_network() {
    // Given — a loopback listener, and `nc` declared as a tool with no `network:` section
    // (`egress: none`); the same probe, unconfined and hermetic
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback listener");
    let port = listener.local_addr().expect("its address").port();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            drop(socket);
        }
    });
    let nc_tool = r#"
  - id: "tools:nc"
    config:
      type: tool
      schema_version: 1
      id: nc
      summary: netcat, declaring no network
      binary: { command: nc, env: NC_BINARY }
"#
    .to_string();
    let probe = format!("nc -z -w 2 127.0.0.1 {port}");
    let uses_nc = "        tool_dep_ids: [\"tools:nc\"]\n";
    let repo = a_repo_with_manifest(&a_manifest(&[
        nc_tool,
        a_target("net:open", false, &probe, uses_nc),
        a_target("net:jailed", true, &probe, uses_nc),
    ]));
    let host = || a_host().with_env("NC_BINARY", Path::new("/usr/bin/nc"));
    let open = repo.build("net:open", host()).await;
    assert_eq!(open.exit_code, 0, "unconfined, the probe connects");

    // When
    let jailed = repo.build("net:jailed", host()).await;

    // Then
    assert_eq!(jailed.exit_code, 1);
}

//! Acceptance: a BUILD.yaml `type: tool` target is a tool definition — inline, or a `ref` into the
//! tool registry — and an action's `tool_dep_ids` resolve through that registry.
//!
//! PRD: `docs/ft/build/1-WIP/PRD-2026-10-31-build-tools-are-tool-defs.md` (`#agent-tools 4/12`).
//!
//! Every test builds a real repository in a tempdir and runs real actions through
//! [`execute_target_with_tools`]. The host is a fixture over the real filesystem whose `PATH` holds
//! only the system directories, so a tool is reachable only because the build resolved it — never
//! because the developer's shell happens to have it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_build::executor::{execute_target_with_tools, BuildRecord, BuildTools, ExecuteOptions};
use tddy_build::graph::BuildGraph;
use tddy_build::plugin::PluginRegistry;
use tddy_build::{load_build_manifest, BuildError, BuildMode};

/// A host whose environment and `PATH` the test declares, over the real filesystem.
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

/// A repository with one `BUILD.yaml`, an empty `<tddyhome>/tools` beside it, and files to build.
struct ARepo {
    root: PathBuf,
    tools_dir: PathBuf,
    _tmp: tempfile::TempDir,
}

fn a_repo_with_manifest(yaml: &str) -> ARepo {
    let tmp = tempfile::tempdir().expect("tempdir");
    let base = std::fs::canonicalize(tmp.path()).expect("canonical tempdir");
    let root = base.join("repo");
    let tools_dir = base.join("tddyhome").join("tools");
    std::fs::create_dir_all(&root).expect("repo");
    std::fs::create_dir_all(&tools_dir).expect("tools dir");
    std::fs::write(root.join("BUILD.yaml"), yaml).expect("BUILD.yaml");
    ARepo {
        root,
        tools_dir,
        _tmp: tmp,
    }
}

impl ARepo {
    /// An executable shell script at `rel` (relative to the repository root).
    fn with_script(self, rel: &str, body: &str) -> Self {
        write_script(&self.root.join(rel), body);
        self
    }

    /// A `<tddyhome>/tools/<file>` override.
    fn with_tool_override(self, file: &str, yaml: &str) -> Self {
        std::fs::write(self.tools_dir.join(file), yaml).expect("tool override");
        self
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.path(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
    }

    /// Load the manifest and build `target` with the repository's tools resolving against `host`.
    async fn build(&self, target: &str, host: AHost) -> Result<BuildRecord, BuildError> {
        let yaml = std::fs::read_to_string(self.path("BUILD.yaml")).expect("BUILD.yaml");
        let manifest = load_build_manifest(&yaml)?;
        let graph = BuildGraph::from_manifests(vec![manifest])?;
        let tools = BuildTools::load(&self.tools_dir, Arc::new(host))?;
        execute_target_with_tools(
            &self.root,
            &graph,
            target,
            &ExecuteOptions::default(),
            BuildMode::Compile,
            &PluginRegistry::new(),
            &tools,
        )
        .await
    }
}

fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("script dir");
    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).expect("script");
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

/// What each action in `record` reports as `cached`, in order.
fn cached_flags(record: &BuildRecord) -> Vec<bool> {
    record.actions.iter().map(|a| a.cached).collect()
}

#[tokio::test]
async fn an_action_using_a_tool_built_by_another_target_builds_that_target_first_and_finds_the_tool_on_path(
) {
    // Given — `app:build` declares no deps: only its tool links it to `tools:stamp-build`
    let repo = a_repo_with_manifest(
        r#"
schema_version: 1
targets:
  - id: "tools:stamp-build"
    actions:
      - id: install
        type: command
        command: ["sh", "-c", "mkdir -p out && cp src/stamp out/stamp && chmod +x out/stamp"]
        inputs: [{ include: ["src/stamp"], root: "." }]
        outputs: [{ path: "out/stamp", kind: file }]
  - id: "tools:stamp"
    config:
      type: tool
      schema_version: 1
      id: stamp
      summary: stamps its input
      binary:
        command: stamp
        target: { target: "tools:stamp-build", path: out/stamp }
  - id: "app:build"
    actions:
      - id: package
        type: command
        command: ["sh", "-c", "echo lib | stamp > app.txt"]
        tool_dep_ids: ["tools:stamp"]
        outputs: [{ path: "app.txt", kind: file }]
"#,
    )
    .with_script("src/stamp", "echo \"STAMPED:$(cat)\"");

    // When
    let record = repo
        .build("app:build", a_host())
        .await
        .expect("the build succeeds");

    // Then
    assert_eq!(record.actions[0].exit_code, 0);
    assert_eq!(repo.read("app.txt"), "STAMPED:lib\n");
    assert_eq!(repo.read("out/stamp"), repo.read("src/stamp"));
}

#[tokio::test]
async fn a_ref_tool_target_resolves_from_the_registry_including_a_tddyhome_override() {
    // Given — `<tddyhome>/tools/rg.yaml` replaces the built-in `rg` with a stub named by its own
    // variable; the build names it only by `ref: rg`
    let repo = a_repo_with_manifest(
        r#"
schema_version: 1
targets:
  - id: "tools:rg"
    config: { type: tool, ref: rg }
  - id: "search:run"
    actions:
      - id: search
        type: command
        command: ["sh", "-c", "rg --version > found.txt"]
        tool_dep_ids: ["tools:rg"]
        outputs: [{ path: "found.txt", kind: file }]
"#,
    )
    .with_script("stubs/rg", "echo OVERRIDDEN-RG")
    .with_tool_override(
        "rg.yaml",
        "schema_version: 1\n\
         id: rg\n\
         summary: a stub ripgrep\n\
         binary: { command: rg, env: STUB_RG }\n",
    );
    let host = a_host().with_env("STUB_RG", &repo.path("stubs/rg"));

    // When
    let record = repo
        .build("search:run", host)
        .await
        .expect("the build succeeds");

    // Then
    assert_eq!(record.actions[0].exit_code, 0);
    assert_eq!(repo.read("found.txt"), "OVERRIDDEN-RG\n");
}

#[tokio::test]
async fn changing_a_tools_binary_invalidates_the_cache_of_every_action_using_it() {
    // Given — two targets whose actions use the same inline tool, built once and then again
    let repo = a_repo_with_manifest(
        r#"
schema_version: 1
targets:
  - id: "tools:stamp"
    config:
      type: tool
      schema_version: 1
      id: stamp
      summary: stamps
      binary: { command: stamp, env: STAMP_BINARY }
  - id: "a:use"
    actions:
      - id: use
        type: command
        command: ["sh", "-c", "stamp > a.txt"]
        tool_dep_ids: ["tools:stamp"]
        outputs: [{ path: "a.txt", kind: file }]
  - id: "b:use"
    actions:
      - id: use
        type: command
        command: ["sh", "-c", "stamp > b.txt"]
        tool_dep_ids: ["tools:stamp"]
        outputs: [{ path: "b.txt", kind: file }]
"#,
    )
    .with_script("stubs/stamp", "echo v1");
    let stamp = repo.path("stubs/stamp");
    let host = || a_host().with_env("STAMP_BINARY", &stamp);
    repo.build("a:use", host()).await.expect("a builds");
    repo.build("b:use", host()).await.expect("b builds");
    let a_unchanged = repo.build("a:use", host()).await.expect("a rebuilds");
    let b_unchanged = repo.build("b:use", host()).await.expect("b rebuilds");

    // When — the tool's binary changes (a different size, so its fingerprint differs)
    write_script(&stamp, "echo version-two");
    let a_after = repo.build("a:use", host()).await.expect("a rebuilds");
    let b_after = repo.build("b:use", host()).await.expect("b rebuilds");

    // Then — the unchanged tool was a hit for both; the changed one a miss for both
    assert_eq!(
        (cached_flags(&a_unchanged), cached_flags(&b_unchanged)),
        (vec![true], vec![true])
    );
    assert_eq!(
        (cached_flags(&a_after), cached_flags(&b_after)),
        (vec![false], vec![false])
    );
    assert_eq!(
        (repo.read("a.txt"), repo.read("b.txt")),
        ("version-two\n".to_string(), "version-two\n".to_string())
    );
}

#[tokio::test]
async fn the_legacy_bin_dir_tool_shape_is_refused_with_a_migration_message() {
    // Given — the pre-`#agent-tools` shape: a directory on PATH
    let repo = a_repo_with_manifest(
        r#"
schema_version: 1
targets:
  - id: "tools:bin"
    config:
      type: tool
      bin_dir: tools/bin
      commands: { greet: greet }
  - id: "uses:tool"
    actions:
      - id: greet
        type: command
        command: ["greet"]
        tool_dep_ids: ["tools:bin"]
"#,
    )
    .with_script("tools/bin/greet", "echo GREETED");

    // When
    let refused = repo
        .build("uses:tool", a_host())
        .await
        .expect_err("the legacy shape is refused");

    // Then — the refusal names the target and what to write instead
    assert_eq!(
        refused.to_string(),
        "manifest error: tool target tools:bin: the {bin_dir, commands} tool shape is no longer \
         supported — declare a tool definition (schema_version, id, summary, binary) or \
         ref: <tool id> instead"
    );
}

//! Acceptance: `tddy-coder --agent` accepts any coding-agent registry id that can take a turn
//! without a terminal, and the interactive selection lists the registry (`#agent-tools 12/12`).
//!
//! Feature doc: `docs/ft/daemon/session-agent-roster.md`.
//!
//! The registry is loaded from a `<tddyhome>/tools` of the test's own holding one new agent,
//! `acme`, declared in YAML alone: a headless turn read by the existing `claude-stream-json`
//! adapter.
//!
//! `#agent-tools 16/19` adds that `--agent <id>` *builds* a backend for every one of those ids —
//! `acme` running a turn through its adapter, and `oz` — and refuses any other id
//! (changeset `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`).

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_coder::agent_registry_cli::{backend_agent_ids, backend_selection_question_from};
use tddy_coder::spec_backend_build::{build_spec_backend, SpecBackendRequest};
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::{CodingBackend, InvokeRequest, SharedBackend};

/// An agent no Rust names: its turn is one `acme -p PROMPT`, read as Claude's stream JSON.
const ACME_YAML: &str = r#"schema_version: 1
id: acme
summary: Acme Code — an agent added as a tool file alone
binary:
  command: acme
  env: TDDY_ACME_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv:
    - "{binary}"
    - { if: model, then: ["--model", "{model}"] }
  env: {}
extensions:
  coding_agent:
    mcp: { kind: unsupported }
    tool_allow: unsupported
    tool_deny: unsupported
    permission_prompt: unsupported
    instructions:
      files: [AGENTS.md]
    hooks: { adapter: unsupported }
    capabilities: [requires_model]
    models:
      default: acme-1
      curated: [acme-1]
    headless:
      argv: ["{binary}", -p, "{prompt}", --output-format, stream-json]
      adapter: claude-stream-json
"#;

#[test]
fn tddy_coder_agent_accepts_any_registry_id_and_the_interactive_selection_lists_the_registry() {
    // Given
    let home = tempfile::tempdir().unwrap();
    let tools = home.path().join("tools");
    std::fs::create_dir_all(&tools).unwrap();
    std::fs::write(tools.join("acme.yaml"), ACME_YAML).unwrap();
    let registry = CodingAgentRegistry::load(&tools).expect("the registry loads");

    // When
    let accepted = backend_agent_ids(&registry);
    let offered: Vec<String> = backend_selection_question_from(&registry)
        .options
        .into_iter()
        .map(|option| option.label)
        .collect();

    // Then — every agent with a headless or ACP mode, `acme` among them, then the stub; the
    // PTY-only `muse` and `warp` are not workflow backends
    let expected: Vec<String> = [
        "acme",
        "claude",
        "claude-acp",
        "codex",
        "codex-acp",
        "cursor",
        "opencode",
        "oz",
        "stub",
    ]
    .iter()
    .map(|id| id.to_string())
    .collect();
    assert_eq!((accepted, offered), (expected.clone(), expected));
}

// ---------------------------------------------------------------------------
// `--agent <id>` builds a backend for any registry agent (`#agent-tools 16/19`)
// ---------------------------------------------------------------------------
//
// Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`. Coding agents are installed
// the stack's way: a YAML under the test's own `<tddyhome>/tools`, and a fixture host whose
// environment names stub binaries through each spec's `binary.env`.

/// A host with nothing of the real machine: the env it is given, and executable files.
struct AFixtureHost {
    env: BTreeMap<String, String>,
}

impl ToolHost for AFixtureHost {
    fn env_var(&self, name: &str) -> Option<String> {
        self.env.get(name).cloned()
    }

    fn path_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }

    fn is_executable_file(&self, path: &Path) -> bool {
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }

    fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        std::fs::canonicalize(path)
    }
}

/// What the stub `acme` prints: one successful Claude stream-JSON turn.
const ACME_TURN: &str =
    r#"{"type":"result","subtype":"success","result":"The plan is ready.","session_id":"acme-1"}"#;

/// A `<tddyhome>` whose `tools/` holds `acme.yaml`, a git checkout to work in, and stubs `acme`
/// (records its argv, prints [`ACME_TURN`]), `oz` and `tddy-tools`.
struct ATddyHome {
    dir: tempfile::TempDir,
}

fn a_tddyhome_with_acme() -> ATddyHome {
    let dir = tempfile::tempdir().expect("tddyhome");
    for sub in ["tools", "bin", "repo", "session", "record"] {
        std::fs::create_dir(dir.path().join(sub)).expect("tddyhome dir");
    }
    std::fs::write(dir.path().join("tools").join("acme.yaml"), ACME_YAML).expect("acme.yaml");
    let git = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(dir.path().join("repo"))
        .status()
        .expect("git runs");
    assert!(git.success(), "the repo is a git checkout");
    let argv_record = dir.path().join("record").join("acme.argv");
    let stubs = [
        (
            "acme",
            format!(
                "#!/bin/sh\nfor a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{}'\nprintf '%s\\n' '{ACME_TURN}'\n",
                argv_record.display()
            ),
        ),
        ("oz", "#!/bin/sh\nexit 0\n".to_string()),
        ("tddy-tools", "#!/bin/sh\n".to_string()),
    ];
    for (name, script) in stubs {
        let path = dir.path().join("bin").join(name);
        std::fs::write(&path, script).expect("write stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    ATddyHome { dir }
}

impl ATddyHome {
    fn tools_dir(&self) -> PathBuf {
        self.dir.path().join("tools")
    }

    fn host(&self) -> Arc<dyn ToolHost> {
        let bin = |name: &str| {
            self.dir
                .path()
                .join("bin")
                .join(name)
                .to_string_lossy()
                .into_owned()
        };
        Arc::new(AFixtureHost {
            env: BTreeMap::from([
                ("TDDY_ACME_CLI".to_string(), bin("acme")),
                ("TDDY_OZ_CLI".to_string(), bin("oz")),
                ("TDDY_TOOLS_BINARY".to_string(), bin("tddy-tools")),
            ]),
        })
    }

    /// What `tddy-coder --agent <agent>` builds on this host, with nothing else asked.
    fn agent(&self, agent: &str) -> anyhow::Result<SharedBackend> {
        build_spec_backend(
            &self.tools_dir(),
            agent,
            self.host(),
            SpecBackendRequest::default(),
        )
    }

    fn a_turn_in_the_repo(&self) -> InvokeRequest {
        InvokeRequest {
            prompt: "Plan the feature".to_string(),
            model: Some("acme-1".to_string()),
            working_dir: Some(self.dir.path().join("repo")),
            session_dir: Some(self.dir.path().join("session")),
            ..InvokeRequest::default()
        }
    }

    fn argv_acme_ran_with(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("record").join("acme.argv"))
            .expect("the acme stub ran")
            .lines()
            .map(str::to_string)
            .collect()
    }
}

#[tokio::test]
async fn tddy_coder_agent_builds_a_backend_for_an_agent_declared_only_in_tddyhome_tools_and_runs_a_turn_through_its_adapter(
) {
    // Given
    let home = a_tddyhome_with_acme();
    let backend = home
        .agent("acme")
        .unwrap_or_else(|e| panic!("`--agent acme` builds a backend: {e:#}"));

    // When
    let response = backend
        .invoke(home.a_turn_in_the_repo())
        .await
        .expect("the turn runs");

    // Then — acme ran as its YAML renders it, and its stdout was read as Claude's stream JSON
    assert_eq!(
        (
            backend.name().to_string(),
            home.argv_acme_ran_with(),
            response.output,
            response.session_id,
        ),
        (
            "acme".to_string(),
            ["-p", "Plan the feature", "--output-format", "stream-json"]
                .iter()
                .map(|arg| arg.to_string())
                .collect::<Vec<_>>(),
            "The plan is ready.".to_string(),
            Some("acme-1".to_string()),
        )
    );
}

#[test]
fn tddy_coder_agent_oz_builds_its_headless_backend() {
    // Given
    let home = a_tddyhome_with_acme();

    // When
    let backend = home
        .agent("oz")
        .unwrap_or_else(|e| panic!("`--agent oz` builds a backend: {e:#}"));

    // Then
    assert_eq!(backend.name(), "oz");
}

#[test]
fn an_unknown_agent_is_refused_listing_the_registry_ids() {
    // Given
    let home = a_tddyhome_with_acme();

    // When
    let refusal = format!(
        "{:#}",
        home.agent("acne")
            .expect_err("an id no registry agent answers to is refused")
    );

    // Then — never run as `claude`; the operator is told what `--agent` takes
    assert_eq!(
        refusal,
        "unknown agent \"acne\": no coding agent of that id can run a workflow turn on this \
         host. Known agents: acme, claude, claude-acp, codex, codex-acp, cursor, opencode, oz, stub"
    );
}

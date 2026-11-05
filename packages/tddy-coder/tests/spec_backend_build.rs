//! `tddy_coder::spec_backend_build` — how `--agent <id>` picks and builds a backend from the
//! coding-agent registry (`#agent-tools 16/19`): the transport rule for a spec declaring both
//! `headless` and `acp`, the refusal of a PTY-only agent, the stub, and the reserved assistant
//! names the model registry is fed.
//!
//! Changeset: `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`. The decided rule: a spec
//! declaring both runs **headless** unless the session or request asks for ACP.
//!
//! The registry is a `<tddyhome>/tools` of the test's own holding `duo`, an agent declared in YAML
//! alone with both a headless and an ACP section. The host is a fixture whose environment names
//! stub binaries; nothing of the real machine resolves.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolHost;
use tddy_coder::agent_registry_cli::reserved_agent_ids;
use tddy_coder::spec_backend_build::{build_spec_backend, spec_backend_for, SpecBackendRequest};
use tddy_coding_agent_specs::CodingAgentRegistry;
use tddy_core::provider_route::HarnessRoute;
use tddy_core::{CodingBackend, SpecTransport};

/// An agent no Rust names, declaring both ways to take a turn without a terminal.
const DUO_YAML: &str = r#"schema_version: 1
id: duo
summary: Duo Code — an agent with both a headless and an ACP mode
binary:
  command: duo
  env: TDDY_DUO_CLI
deps: [tddy-tools]
reads: []
run:
  transport: pty
  argv: ["{binary}"]
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
    capabilities: []
    models:
      default: duo-1
      curated: [duo-1]
    headless:
      argv: ["{binary}", -p, "{prompt}", --output-format, stream-json]
      adapter: claude-stream-json
    acp:
      command: ["{binary}", acp]
      env: {}
"#;

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

/// A `<tddyhome>` whose `tools/` holds `duo.yaml`, with stubs `duo`, `muse` and `tddy-tools`.
struct ATddyHome {
    dir: tempfile::TempDir,
}

fn a_tddyhome_with_duo() -> ATddyHome {
    let dir = tempfile::tempdir().expect("tddyhome");
    for sub in ["tools", "bin"] {
        std::fs::create_dir(dir.path().join(sub)).expect("tddyhome dir");
    }
    std::fs::write(dir.path().join("tools").join("duo.yaml"), DUO_YAML).expect("duo.yaml");
    for name in ["duo", "muse", "tddy-tools"] {
        let path = dir.path().join("bin").join(name);
        std::fs::write(&path, "#!/bin/sh\nexec cat > /dev/null\n").expect("write stub");
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
                ("TDDY_DUO_CLI".to_string(), bin("duo")),
                ("TDDY_MUSE_CLI".to_string(), bin("muse")),
                ("TDDY_TOOLS_BINARY".to_string(), bin("tddy-tools")),
            ]),
        })
    }

    fn transport_of(&self, agent: &str, asked: Option<SpecTransport>) -> SpecTransport {
        spec_backend_for(&self.tools_dir(), agent, self.host(), asked)
            .unwrap_or_else(|e| panic!("`{agent}` builds a spec backend: {e:#}"))
            .transport()
    }

    fn refusal_of(&self, agent: &str, request: SpecBackendRequest) -> String {
        format!(
            "{:#}",
            build_spec_backend(&self.tools_dir(), agent, self.host(), request)
                .expect_err("the agent is refused")
        )
    }
}

/// A route onto an Anthropic-compatible account — any route will do: the stub takes none.
fn an_anthropic_account_route() -> HarnessRoute {
    HarnessRoute {
        env: vec![(
            "ANTHROPIC_BASE_URL".to_string(),
            "https://llm.example.test".to_string(),
        )],
        ..HarnessRoute::default()
    }
}

#[test]
fn an_agent_declaring_both_headless_and_acp_is_built_headless_when_nothing_asks_for_acp() {
    // Given
    let home = a_tddyhome_with_duo();

    // When
    let transport = home.transport_of("duo", None);

    // Then
    assert_eq!(transport, SpecTransport::Headless);
}

#[test]
fn an_agent_declaring_both_headless_and_acp_is_built_over_acp_when_the_session_asks_for_it() {
    // Given
    let home = a_tddyhome_with_duo();

    // When
    let transport = home.transport_of("duo", Some(SpecTransport::Acp));

    // Then
    assert_eq!(transport, SpecTransport::Acp);
}

#[test]
fn a_pty_only_agent_is_refused_naming_it_and_never_built_as_another_agent() {
    // Given
    let home = a_tddyhome_with_duo();

    // When
    let refusal = home.refusal_of("muse", SpecBackendRequest::default());

    // Then
    assert_eq!(
        refusal,
        "agent `muse` declares no `coding_agent.headless` or `coding_agent.acp` section, so it \
         cannot run a workflow turn"
    );
}

#[test]
fn the_stub_agent_builds_the_simulated_response_backend() {
    // Given
    let home = a_tddyhome_with_duo();

    // When
    let backend = build_spec_backend(
        &home.tools_dir(),
        "stub",
        home.host(),
        SpecBackendRequest::default(),
    )
    .expect("`stub` builds");

    // Then
    assert_eq!(backend.name(), "stub");
}

#[test]
fn the_stub_agent_refuses_a_provider_route() {
    // Given
    let home = a_tddyhome_with_duo();

    // When
    let refusal = home.refusal_of(
        "stub",
        SpecBackendRequest {
            provider_route: Some(an_anthropic_account_route()),
            ..SpecBackendRequest::default()
        },
    );

    // Then
    assert_eq!(
        refusal,
        "the stub backend cannot run on a provider account: it makes no provider call"
    );
}

#[test]
fn the_reserved_assistant_names_are_every_registry_agent_then_stub() {
    // Given
    let home = a_tddyhome_with_duo();
    let registry = CodingAgentRegistry::load(&home.tools_dir()).expect("the registry loads");

    // When
    let reserved = reserved_agent_ids(&registry);

    // Then — PTY-only `muse` and `warp` too: each is a session type `--agent` already means
    let expected: Vec<String> = [
        "claude",
        "claude-acp",
        "codex",
        "codex-acp",
        "cursor",
        "duo",
        "muse",
        "opencode",
        "oz",
        "warp",
        "stub",
    ]
    .iter()
    .map(|id| id.to_string())
    .collect();
    assert_eq!(reserved, expected);
}

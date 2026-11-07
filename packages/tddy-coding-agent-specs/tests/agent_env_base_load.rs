//! Acceptance: the base set of every agent's environment is YAML, built in and overridable through
//! `<tddyhome>/tools/agent-env/base.yaml`, and can never admit a `TDDY_*` variable
//! (`#agent-tools 28/28`).
//!
//! PRD: `docs/ft/daemon/1-WIP/PRD-2026-11-06-agent-env-allow-list.md`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pretty_assertions::assert_eq;
use tddy_actions::tool::ToolDef;
use tddy_coding_agent_specs::{
    AgentEnvBase, AgentEnvBaseError, CodingAgentRegistry, CodingAgentSpec, RegistryError,
    AGENT_ENV_BASE_OVERRIDE,
};

/// A `<tddyhome>/tools` directory.
struct AToolsDir {
    dir: tempfile::TempDir,
}

fn a_tools_dir() -> AToolsDir {
    AToolsDir {
        dir: tempfile::tempdir().expect("a tools dir"),
    }
}

impl AToolsDir {
    /// With an operator's base-set override listing `entries` (a YAML flow list).
    fn overriding_the_base_set_with(self, entries: &str) -> Self {
        let file = self.override_file();
        std::fs::create_dir_all(file.parent().expect("a parent")).expect("the override dir");
        std::fs::write(&file, format!("schema_version: 1\nbase: {entries}\n"))
            .expect("the override file");
        self
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn override_file(&self) -> PathBuf {
        self.dir.path().join(AGENT_ENV_BASE_OVERRIDE)
    }
}

fn names(entries: &[&str]) -> Vec<String> {
    entries.iter().map(|entry| entry.to_string()).collect()
}

fn a_lark_spec_passing(pass_env: &str) -> CodingAgentSpec {
    let def: ToolDef = serde_yaml::from_str(&format!(
        r#"schema_version: 1
id: lark
summary: a coding agent
binary:
  command: lark
deps: []
run:
  transport: pty
  argv: ["{{binary}}"]
extensions:
  coding_agent:
    mcp: {{ kind: file, flag: --mcp-config, format: claude-mcp-json }}
    tool_allow: {{ flag: --allowedTools }}
    tool_deny: {{ flag: --disallowedTools }}
    permission_prompt: {{ flag: --permission-prompt-tool }}
    instructions: {{ files: [CLAUDE.md] }}
    hooks: {{ adapter: claude-settings-local }}
    models: {{ default: opus, curated: [opus] }}
    pass_env: {pass_env}
"#
    ))
    .expect("the tool file deserializes");
    CodingAgentSpec::of(&def).expect("a valid coding agent")
}

#[test]
fn the_builtin_base_set_is_the_approved_one() {
    // When
    let base = AgentEnvBase::builtin();

    // Then — guard: approved by the developer on 2026-11-06
    assert_eq!(
        base.entries(),
        names(&[
            "PATH",
            "HOME",
            "USER",
            "LOGNAME",
            "SHELL",
            "TERM",
            "LANG",
            "LC_*",
            "TMPDIR",
            "TZ",
            "SSH_AUTH_SOCK",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "no_proxy",
            "XDG_*",
        ])
        .as_slice()
    );
}

#[test]
fn a_registry_with_no_override_keeps_the_builtin_base_set() {
    // When
    let registry = CodingAgentRegistry::load(a_tools_dir().path()).expect("the built-ins load");

    // Then — guard
    assert_eq!(registry.agent_env_base(), &AgentEnvBase::builtin());
}

#[test]
fn an_operator_override_of_the_base_set_is_honoured() {
    // Given — an operator who keeps a corporate CA bundle variable and drops the proxies
    let tools = a_tools_dir().overriding_the_base_set_with("[PATH, HOME, SSL_CERT_FILE]");

    // When
    let registry = CodingAgentRegistry::load(tools.path()).expect("the override loads");

    // Then
    assert_eq!(
        registry.agent_env_base().entries(),
        names(&["PATH", "HOME", "SSL_CERT_FILE"]).as_slice()
    );
    assert_eq!(registry.agent_env_base().file(), tools.override_file());
}

#[test]
fn a_tddy_entry_in_the_base_set_fails_loading_naming_the_file() {
    for entry in ["TDDY_SOCKET", "\"TDDY_*\"", "\"*\""] {
        // Given
        let tools = a_tools_dir().overriding_the_base_set_with(&format!("[PATH, {entry}]"));

        // When
        let refused = AgentEnvBase::load(tools.path()).unwrap_err();

        // Then
        let AgentEnvBaseError::Invalid { file, message } = refused;
        assert_eq!(file, tools.override_file(), "{entry}");
        assert!(
            message.contains(entry.trim_matches('"')),
            "{entry}: {message}"
        );
    }
}

#[test]
fn a_registry_whose_base_set_is_refused_does_not_load() {
    // Given
    let tools = a_tools_dir().overriding_the_base_set_with("[PATH, TDDY_SOCKET]");

    // When
    let refused = CodingAgentRegistry::load(tools.path()).unwrap_err();

    // Then
    assert!(
        matches!(refused, RegistryError::AgentEnvBase(_)),
        "{refused:?}"
    );
}

#[test]
fn an_agent_env_is_the_base_set_then_the_spec_pass_env_then_the_launch_env() {
    // Given
    let base = AgentEnvBase::parse(
        Path::new("agent-env/base.yaml"),
        "schema_version: 1\nbase: [PATH, \"LC_*\"]\n",
    )
    .expect("the base set parses");
    let spec = a_lark_spec_passing("[LARK_API_KEY]");
    let daemon: Vec<(OsString, OsString)> = [
        ("PATH", "/usr/bin"),
        ("LC_ALL", "C"),
        ("LARK_API_KEY", "sk-lark"),
        ("HOME", "/home/dev"),
        ("CLAUDE_CODE_CHILD_SESSION", "1"),
        ("TDDY_DAEMON_CONFIG", "/etc/tddy/daemon.yaml"),
    ]
    .into_iter()
    .map(|(name, value)| (OsString::from(name), OsString::from(value)))
    .collect();

    // When
    let env = base.agent_env(
        &spec,
        daemon,
        &[(
            "ROUTE_BASE_URL".to_string(),
            "https://route.example".to_string(),
        )],
    );

    // Then — HOME is not in this base set, so it does not pass either
    assert_eq!(
        env,
        vec![
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("LARK_API_KEY".to_string(), "sk-lark".to_string()),
            (
                "ROUTE_BASE_URL".to_string(),
                "https://route.example".to_string()
            ),
        ]
    );
}

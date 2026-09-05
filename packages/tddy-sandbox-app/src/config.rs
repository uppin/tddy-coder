//! `--config <yaml>` schema for `tddy-sandbox-app`, plus the helper that resolves a session's
//! active specialized-agent defs from named agents + inline defs + the on-disk `agents_dir` pool.
//!
//! Every config field is optional and a CLI flag always overrides its config counterpart (see
//! `main.rs`). `subagents` carries full inline [`SpecializedAgentDef`]s — the same schema as
//! `<tddyhome>/agents/*.yaml` — so a whole session's subagent wiring (e.g. pointing an explorer
//! agent at a local Ollama server) can live in one file with no separate agents dir.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use tddy_discovery::agent_def::SpecializedAgentDef;

/// Optional YAML config for a sandboxed Claude session.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxAppConfig {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// `mounted` or `managed` — see `--codebase-mode`.
    #[serde(default)]
    pub codebase_mode: Option<String>,
    #[serde(default)]
    pub claude_binary: Option<String>,
    #[serde(default)]
    pub cursor_binary: Option<String>,
    #[serde(default)]
    pub tddy_tools_path: Option<String>,
    #[serde(default)]
    pub sandbox_runner_path: Option<String>,
    #[serde(default)]
    pub session_base: Option<PathBuf>,
    #[serde(default)]
    pub claude_home_dir: Option<PathBuf>,
    #[serde(default)]
    pub cursor_home_dir: Option<PathBuf>,
    /// `--codebase-mode sandboxed` only: the **base** holding this host's build homes — one
    /// directory per repository, each the jail's `$HOME` for that checkout's build and its
    /// dependency caches. Not itself any jail's `$HOME`: one home shared by every repository would
    /// let an unaudited build leave a `~/.cargo/config.toml` behind for the next session's *other*
    /// checkout to build with. Shared across sessions within one repository — see
    /// `--codebase-home-dir`.
    #[serde(default)]
    pub codebase_home_dir: Option<PathBuf>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub agents_dir: Option<PathBuf>,
    /// Named specialized agents resolved against `agents_dir` (same as repeating
    /// `--specialized-agent`).
    #[serde(default)]
    pub specialized_agents: Vec<String>,
    /// Full inline specialized-agent defs. Each entry both **defines** and **activates** its
    /// agent, and overrides an `agents_dir` def of the same `name` — so an agent can be
    /// re-pointed at another endpoint without a separate agents-dir file.
    #[serde(default)]
    pub subagents: Vec<SpecializedAgentDef>,
    /// Extra args forwarded verbatim to the in-jail `claude` (before any `-- <args>` given on the
    /// CLI, which are appended after these).
    #[serde(default)]
    pub claude_args: Vec<String>,
    /// `RUST_LOG` for the in-jail `tddy-tools --mcp` server, whose logs (including specialized
    /// subagent HTTP activity) are persisted to `<session-dir>/egress/tddy-tools.mcp.log`. When
    /// unset, the runner picks a default that captures subagent turns.
    #[serde(default)]
    pub mcp_log_level: Option<String>,
}

impl SandboxAppConfig {
    /// Load and parse a config file. A malformed file (invalid YAML, unknown field, or a
    /// malformed inline subagent def) is a hard error — not a silent default.
    pub fn load(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("read sandbox config {}", path.display()))?;
        serde_yaml::from_str(&contents)
            .with_context(|| format!("parse sandbox config {}", path.display()))
    }
}

/// Resolve the final active specialized-agent def set for a session.
///
/// The pool is `agents_dir/*.yaml`, then each inline def overlaid by `name` (inline
/// wins). The active set is every inline def (declaring an inline subagent activates it) plus each
/// `named` agent, de-duplicated with first-seen order preserved. A `named` agent that resolves
/// against nothing in the pool is a hard error — not a silently-dropped entry.
///
/// Only *called* from the macOS in-process spawn path, which must resolve each subagent's full def
/// (model, base_url, tools) to wire it into the in-jail `tddy-tools --mcp`. The Linux daemon-assisted
/// path instead forwards the requested agent *names* over `StartSessionRequest.specialized_agents`
/// and lets the daemon resolve them against its own `<tddyhome>/agents`, so it never calls this. The
/// resolution logic itself is platform-agnostic, so this stays compiled and unit-tested on all
/// platforms; the allow suppresses the resulting dead-code lint on non-macOS builds.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn resolve_session_agents(
    named: &[String],
    inline: &[SpecializedAgentDef],
    agents_dir: &Path,
) -> Result<Vec<SpecializedAgentDef>> {
    let mut pool = tddy_discovery::agent_def::resolve_agent_defs(agents_dir);
    for def in inline {
        match pool.iter_mut().find(|d| d.name == def.name) {
            Some(existing) => *existing = def.clone(),
            None => pool.push(def.clone()),
        }
    }

    let mut active: Vec<String> = Vec::new();
    for def in inline {
        if !active.iter().any(|n| n == &def.name) {
            active.push(def.name.clone());
        }
    }
    for name in named {
        if !active.iter().any(|n| n == name) {
            active.push(name.clone());
        }
    }

    let mut out = Vec::with_capacity(active.len());
    for name in &active {
        let def = pool.iter().find(|d| &d.name == name).ok_or_else(|| {
            anyhow::anyhow!(
                "specialized agent '{name}' not found (not inline, and not present under {})",
                agents_dir.display()
            )
        })?;
        out.push(def.clone());
    }
    if let Some(def) = out.iter().find(|def| shell_is_taken_and_bound(def)) {
        anyhow::bail!(
            "specialized agent '{}' both replaces Shell and binds SHELL, which this host cannot \
             serve: every Shell dispatch is rejected at the host boundary (see \
             `bridge::AppToolHandler::policy_rejects`), including the ones this agent makes for \
             itself — so the tool would be withdrawn from the main agent and unusable by its \
             replacement. Drop SHELL from its `tools:`, or Shell from its `replaces:`",
            def.name
        );
    }
    Ok(out)
}

/// Whether `def` takes `Shell` over from the main agent **and** binds `SHELL` for its own loop.
///
/// The host relay carries no caller identity — `HostToolHandler::execute` is handed a session id, a
/// tool name and args — so a `Shell` dispatch made by this agent is indistinguishable from one made
/// by the main agent, and the host rejects both. Refusing the pairing here is the honest half of
/// that: the combination cannot work, so it is named at config time rather than failing per call.
fn shell_is_taken_and_bound(def: &SpecializedAgentDef) -> bool {
    let takes_shell = tddy_discovery::subagent::normalize_replaced_tools(&def.replaces)
        .iter()
        .any(|tool| tool == "Shell");
    let binds_shell = def
        .tools
        .iter()
        .any(|tool| matches!(tool, tddy_discovery::agent_def::SubagentTool::Shell));
    takes_shell && binds_shell
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_AGENTS_DIR: &str = "/nonexistent-agents-dir-for-tests";

    fn an_inline_explorer_yaml() -> &'static str {
        "\
model: claude-opus-4-8
codebase_mode: managed
subagents:
  - name: explorer
    model: qwen2.5-coder:7b
    base_url: http://localhost:11434
    replaces: [Grep, Glob]
claude_args:
  - --add-dir
  - /extra
"
    }

    /// A config carrying an inline subagent def parses into the expected fields, the def included.
    #[test]
    fn parses_a_config_with_an_inline_subagent_def() {
        // Given / When
        let cfg: SandboxAppConfig =
            serde_yaml::from_str(an_inline_explorer_yaml()).expect("config must parse");

        // Then
        assert_eq!(cfg.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(cfg.codebase_mode.as_deref(), Some("managed"));
        assert_eq!(cfg.subagents.len(), 1);
        assert_eq!(cfg.subagents[0].name, "explorer");
        assert_eq!(cfg.subagents[0].base_url, "http://localhost:11434");
        assert_eq!(cfg.subagents[0].model, "qwen2.5-coder:7b");
        assert_eq!(cfg.claude_args, vec!["--add-dir", "/extra"]);
    }

    /// An unknown top-level key is rejected (`deny_unknown_fields`) rather than silently ignored.
    #[test]
    fn rejects_unknown_config_keys() {
        // Given / When
        let result: Result<SandboxAppConfig, _> = serde_yaml::from_str("bogus_key: 1\n");

        // Then
        assert!(result.is_err(), "an unknown top-level key must be rejected");
    }

    /// An empty config parses to all-default (nothing forced) — the CLI supplies every value.
    #[test]
    fn empty_config_is_all_default() {
        // Given / When
        let cfg: SandboxAppConfig = serde_yaml::from_str("{}\n").expect("empty config must parse");

        // Then
        assert_eq!(cfg, SandboxAppConfig::default());
    }

    /// An inline subagent def both defines and activates its agent — it is usable with no
    /// `--specialized-agent` flag naming it, and with nothing under `agents_dir`.
    #[test]
    fn inline_subagent_def_activates_the_agent_it_defines() {
        // Given
        let cfg: SandboxAppConfig =
            serde_yaml::from_str(an_inline_explorer_yaml()).expect("config must parse");

        // When
        let defs = resolve_session_agents(
            &cfg.specialized_agents,
            &cfg.subagents,
            Path::new(NO_AGENTS_DIR),
        )
        .expect("inline def must resolve");

        // Then
        assert_eq!(defs.len(), 1, "the inline explorer must be active");
        assert_eq!(defs[0].name, "explorer");
        assert_eq!(defs[0].base_url, "http://localhost:11434");
    }

    /// A named agent (no inline def) resolves from the agents directory — the only pool there is,
    /// now that no def is compiled in.
    #[test]
    fn a_named_agent_resolves_from_the_agents_directory() {
        // Given
        let agents_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            agents_dir.path().join("explorer.yaml"),
            "name: explorer\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n",
        )
        .expect("write agent def");

        // When
        let defs = resolve_session_agents(&["explorer".to_string()], &[], agents_dir.path())
            .expect("a def in the agents dir must resolve by name");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "explorer");
        assert_eq!(defs[0].base_url, "http://localhost:11434");
    }

    /// An agent that takes `Shell` over from the main agent **and** binds `SHELL` for itself cannot
    /// be served: the host rejects every `Shell` dispatch, including the ones that agent makes, so
    /// the tool would be withdrawn from the main agent and unusable by its replacement. The pairing
    /// is refused where the session is configured, naming the def.
    #[test]
    fn refuses_an_agent_that_both_replaces_shell_and_binds_it() {
        // Given
        let agents_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            agents_dir.path().join("commander.yaml"),
            "name: commander\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n\
             tools: [SHELL]\nreplaces: [Shell]\n",
        )
        .expect("write agent def");

        // When
        let result = resolve_session_agents(&["commander".to_string()], &[], agents_dir.path());

        // Then
        let err = result.expect_err("an agent that replaces Shell and binds it must be refused");
        assert!(
            err.to_string().contains("commander"),
            "the error must name the def that cannot be served; got: {err}"
        );
    }

    /// Replacing `Shell` without binding it is the ordinary no-bash-mode agent: it authors session
    /// actions instead of running commands, and it resolves.
    #[test]
    fn resolves_an_agent_that_replaces_shell_without_binding_it() {
        // Given
        let agents_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            agents_dir.path().join("author.yaml"),
            "name: author\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n\
             tools: [READ]\nreplaces: [Shell]\n",
        )
        .expect("write agent def");

        // When
        let defs = resolve_session_agents(&["author".to_string()], &[], agents_dir.path())
            .expect("replacing Shell without binding it must resolve");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "author");
    }

    /// Binding `SHELL` without replacing it is equally fine: the agent runs commands through the
    /// same relay the main agent does, and nothing is withdrawn.
    #[test]
    fn resolves_an_agent_that_binds_shell_without_replacing_it() {
        // Given
        let agents_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            agents_dir.path().join("builder.yaml"),
            "name: builder\nmodel: qwen2.5-coder:7b\nbase_url: http://localhost:11434\n\
             tools: [SHELL]\n",
        )
        .expect("write agent def");

        // When
        let defs = resolve_session_agents(&["builder".to_string()], &[], agents_dir.path())
            .expect("binding SHELL without replacing it must resolve");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "builder");
    }

    /// A named agent that resolves against neither the inline defs nor `agents_dir` is a hard
    /// error that names the offending agent.
    #[test]
    fn unknown_named_agent_is_an_error() {
        // Given / When
        let result = resolve_session_agents(&["ghost".to_string()], &[], Path::new(NO_AGENTS_DIR));

        // Then
        let err = result.expect_err("an unresolvable name must be rejected");
        assert!(
            err.to_string().contains("ghost"),
            "the error must name the unresolvable agent; got: {err}"
        );
    }

    /// Empty named + empty inline resolves to no defs (not an error) — a plain session with no
    /// specialized agents.
    #[test]
    fn empty_selection_resolves_to_no_defs() {
        // Given / When
        let defs = resolve_session_agents(&[], &[], Path::new(NO_AGENTS_DIR))
            .expect("empty selection must not error");

        // Then
        assert!(defs.is_empty());
    }

    // ─── tool replacement ────────────────────────────────────────────────────────
    //
    // A def's `replaces:` is withdrawn from the main agent and nothing else happens — no def is
    // the session's "action author" or "coder", and nothing validates that a replaced tool is
    // backed by a matching binding (docs/ft/daemon/session-agent-roster.md § Tool replacement,
    // without behaviour). What is left to check here is that the declaration parses.

    /// The documented agent-driven config parses: a gemma def replacing Shell is the whole
    /// no-bash opt-in — no dedicated flag fields exist (unknown keys are rejected).
    #[test]
    fn parses_the_shell_replacing_gemma_config() {
        // Given / When
        let cfg: SandboxAppConfig = serde_yaml::from_str(
            "\
subagents:
  - name: action-author
    model: gemma4:e4b-mlx
    base_url: http://localhost:11434
    replaces: [Shell]
",
        )
        .expect("config must parse");

        // Then
        assert_eq!(cfg.subagents[0].model, "gemma4:e4b-mlx");
        assert_eq!(cfg.subagents[0].replaces, vec!["Shell"]);
        let flag_style: Result<SandboxAppConfig, _> = serde_yaml::from_str("no_bash: true\n");
        assert!(
            flag_style.is_err(),
            "the retired flag field must be rejected as an unknown key"
        );
    }
}

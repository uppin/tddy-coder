//! `--config <yaml>` schema for `tddy-sandbox-app`, plus the helper that resolves a session's
//! active specialized-agent defs from named agents + inline defs + the on-disk `agents_dir` pool.
//!
//! Every config field is optional and a CLI flag always overrides its config counterpart (see
//! `main.rs`). `subagents` carries full inline [`SpecializedAgentDef`]s — the same schema as
//! `<tddyhome>/agents/*.yaml` — so a whole session's subagent wiring (e.g. pointing the
//! `fastcontext` role at a local Ollama server) can live in one file with no separate agents dir.

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
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default)]
    pub agents_dir: Option<PathBuf>,
    /// Named specialized agents resolved against the builtins + `agents_dir` (same as repeating
    /// `--specialized-agent`).
    #[serde(default)]
    pub specialized_agents: Vec<String>,
    /// Full inline specialized-agent defs. Each entry both **defines** and **activates** its
    /// agent, and overrides a builtin or `agents_dir` def of the same `name` — so `fastcontext`
    /// can be re-pointed at Ollama without a separate agents-dir file.
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
/// The pool is `builtins + agents_dir/*.yaml`, then each inline def overlaid by `name` (inline
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
                "specialized agent '{name}' not found (not a builtin, not inline, and not present \
                 under {})",
                agents_dir.display()
            )
        })?;
        out.push(def.clone());
    }
    Ok(out)
}

/// Model-facing subagent tool that must be bound to serve a replaced mutation tool. Read-side
/// replacements (`Grep`/`Glob`/`SemanticSearch`) are deliberately unmapped: the default
/// READ/GLOB/GREP loop serves those (fastcontext replaces `SemanticSearch` without a same-named
/// internal tool today).
fn required_binding_for(replaced: &str) -> Option<tddy_discovery::agent_def::SubagentTool> {
    use tddy_discovery::agent_def::SubagentTool;
    match replaced {
        "Write" => Some(SubagentTool::Write),
        "StrReplace" => Some(SubagentTool::StrReplace),
        "Delete" => Some(SubagentTool::Delete),
        _ => None,
    }
}

/// Validate the session's tool replacements before spawn — every restriction is declared on the
/// agent defs themselves (`replaces:`), not on mode flags (docs/ft/coder/no-bash-mode.md):
///
/// - At most one def may replace `Shell` (that def becomes the session's action author).
/// - A def replacing a mutation tool (`Write`/`StrReplace`/`Delete`) must bind the matching
///   internal tool (`WRITE`/`STR_REPLACE`/`DELETE`) — otherwise the session could never perform
///   that operation at all, which must fail here rather than at the first delegated edit.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn validate_tool_replacements(defs: &[SpecializedAgentDef]) -> Result<()> {
    let shell_replacers: Vec<&str> = defs
        .iter()
        .filter(|def| {
            tddy_discovery::subagent::normalize_replaced_tools(&def.replaces)
                .iter()
                .any(|t| t == "Shell")
        })
        .map(|def| def.name.as_str())
        .collect();
    if shell_replacers.len() > 1 {
        anyhow::bail!(
            "only one subagent may replace Shell (it becomes the session's action author); got: {}",
            shell_replacers.join(", ")
        );
    }

    for def in defs {
        for replaced in tddy_discovery::subagent::normalize_replaced_tools(&def.replaces) {
            let Some(required) = required_binding_for(&replaced) else {
                continue;
            };
            if !def.tools.contains(&required) {
                anyhow::bail!(
                    "subagent '{}' replaces {replaced} but does not bind the matching internal \
                     tool — add `tools: [READ, GLOB, GREP, WRITE, STR_REPLACE, DELETE]` (or at \
                     least {replaced:?}'s counterpart) to its def",
                    def.name
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_AGENTS_DIR: &str = "/nonexistent-agents-dir-for-tests";

    fn ollama_fastcontext_yaml() -> &'static str {
        "\
model: claude-opus-4-8
codebase_mode: managed
subagents:
  - name: fastcontext
    model: hf.co/mitkox/FastContext-1.0-4B-SFT-Q4_K_M-GGUF:Q4_K_M
    base_url: http://localhost:11434
    replaces: [Grep, Glob]
claude_args:
  - --add-dir
  - /extra
"
    }

    /// The documented ollama-fastcontext config parses into the expected fields, including the
    /// inline subagent def re-pointed at the local Ollama server.
    #[test]
    fn parses_the_ollama_fastcontext_config() {
        // Given / When
        let cfg: SandboxAppConfig =
            serde_yaml::from_str(ollama_fastcontext_yaml()).expect("config must parse");

        // Then
        assert_eq!(cfg.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(cfg.codebase_mode.as_deref(), Some("managed"));
        assert_eq!(cfg.subagents.len(), 1);
        assert_eq!(cfg.subagents[0].name, "fastcontext");
        assert_eq!(cfg.subagents[0].base_url, "http://localhost:11434");
        assert_eq!(
            cfg.subagents[0].model,
            "hf.co/mitkox/FastContext-1.0-4B-SFT-Q4_K_M-GGUF:Q4_K_M"
        );
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

    /// An inline subagent def both defines and activates its agent, overriding the same-named
    /// builtin — so `fastcontext` comes back pointed at Ollama with no `--specialized-agent` flag.
    #[test]
    fn inline_subagent_def_activates_and_overrides_builtin() {
        // Given
        let cfg: SandboxAppConfig =
            serde_yaml::from_str(ollama_fastcontext_yaml()).expect("config must parse");

        // When
        let defs = resolve_session_agents(
            &cfg.specialized_agents,
            &cfg.subagents,
            Path::new(NO_AGENTS_DIR),
        )
        .expect("inline def must resolve");

        // Then
        assert_eq!(defs.len(), 1, "the inline fastcontext must be active");
        assert_eq!(defs[0].name, "fastcontext");
        assert_eq!(
            defs[0].base_url, "http://localhost:11434",
            "the inline Ollama base_url must override the builtin default"
        );
    }

    /// A named builtin (no inline def) resolves via the builtin pool, unchanged.
    #[test]
    fn named_builtin_resolves_without_inline_def() {
        // Given / When
        let defs =
            resolve_session_agents(&["fastcontext".to_string()], &[], Path::new(NO_AGENTS_DIR))
                .expect("the builtin must resolve by name");

        // Then
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "fastcontext");
        assert_eq!(
            defs[0].base_url, "http://localhost:30000",
            "an unmodified builtin keeps its shipped base_url"
        );
    }

    /// A named agent that resolves against neither builtins, inline defs, nor `agents_dir` is a
    /// hard error that names the offending agent.
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

    // ─── tool-replacement validation (agent-driven no-bash / no-write) ───────────
    //
    // Feature: docs/ft/coder/no-bash-mode.md — every restriction is declared on the defs
    // themselves via `replaces:`; there are no mode flags.

    fn def_with(name: &str, replaces: &str, tools: Option<&str>) -> SpecializedAgentDef {
        let tools_line = tools.map(|t| format!("tools: {t}\n")).unwrap_or_default();
        serde_yaml::from_str(&format!(
            "name: {name}\nmodel: m\nbase_url: http://localhost:11434\nreplaces: {replaces}\n{tools_line}"
        ))
        .expect("def must parse")
    }

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

    /// A single Shell replacer (the action author) validates.
    #[test]
    fn a_single_shell_replacer_is_accepted() {
        let defs = vec![def_with("action-author", "[Shell]", None)];
        validate_tool_replacements(&defs).expect("one Shell replacer must be accepted");
    }

    /// Two defs both replacing Shell is ambiguous — which one authors actions? — and rejected
    /// before spawn with both names in the error.
    #[test]
    fn two_shell_replacers_are_rejected() {
        let defs = vec![
            def_with("author-a", "[Shell]", None),
            def_with("author-b", "[Shell]", None),
        ];
        let err = validate_tool_replacements(&defs)
            .expect_err("two Shell replacers must be rejected");
        assert!(
            err.to_string().contains("author-a") && err.to_string().contains("author-b"),
            "the error must name both defs; got: {err}"
        );
    }

    /// A def replacing `Write` without binding the internal WRITE tool would leave the session
    /// unable to edit anything — rejected before spawn, with the fix in the error.
    #[test]
    fn a_write_replacer_without_a_write_binding_is_rejected() {
        let defs = vec![def_with("coder", "[Write, StrReplace, Delete]", None)];
        let err = validate_tool_replacements(&defs)
            .expect_err("a write replacer without WRITE must be rejected");
        assert!(
            err.to_string().contains("WRITE"),
            "the error must show the tools to add; got: {err}"
        );
    }

    /// A write-capable coder def replacing the mutation tools validates.
    #[test]
    fn a_write_capable_coder_replacing_mutation_tools_is_accepted() {
        let defs = vec![def_with(
            "coder",
            "[Write, StrReplace, Delete]",
            Some("[READ, GLOB, GREP, WRITE, STR_REPLACE, DELETE]"),
        )];
        validate_tool_replacements(&defs).expect("a write-capable coder must be accepted");
    }

    /// Read-side replacements need no matching internal binding — the builtin fastcontext
    /// (replaces Grep/Glob/SemanticSearch with a READ/GLOB/GREP loop) must keep validating.
    #[test]
    fn read_side_replacements_need_no_matching_binding() {
        let defs = vec![tddy_discovery::agent_def::builtin_fastcontext_def()];
        validate_tool_replacements(&defs).expect("fastcontext must keep validating");
    }
}

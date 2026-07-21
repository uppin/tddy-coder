//! Claude CLI sandbox recipe — reads, copies, policy, MCP argv, env overlays.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tddy_sandbox::builder::{CopySpec, MachPolicy, PolicySpec};
use tddy_sandbox::{process_exec_reads, workspace_exec_tool_names};

/// Read grants for a sandboxed Claude binary (baseline + toolchain + binary deps).
pub fn process_claude_exec_reads(claude_binary: &Path) -> Vec<tddy_sandbox::ReadSpec> {
    process_exec_reads(claude_binary)
}

/// Seed `claude_home_dir/.claude/.credentials.json` from the real host `~/.claude` once, so a
/// **persistent** jail home can authenticate on its first run. Never overwrites an existing file —
/// the jail may have since refreshed its own token, and the host copy must not clobber it on later
/// restarts. This is the persistent-home counterpart to [`claude_credentials_copies`] (which
/// re-copies every session and is only correct for an ephemeral per-session home).
pub fn seed_claude_credentials(claude_home_dir: &Path) -> Result<()> {
    let dest_dir = claude_home_dir.join(".claude");
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("create persistent claude home {}", dest_dir.display()))?;
    let dest = dest_dir.join(".credentials.json");
    if dest.exists() {
        return Ok(());
    }
    let Some(host_home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Ok(());
    };
    let src = host_home.join(".claude").join(".credentials.json");
    if !src.is_file() {
        return Ok(());
    }
    std::fs::copy(&src, &dest)
        .with_context(|| format!("seed credentials {} -> {}", src.display(), dest.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Mirror the host's self-managed install layout
/// (`$HOME/.local/bin/claude` -> `$HOME/.local/share/claude/versions/<version>` -> real binary)
/// inside the persistent jail home, so Claude's own startup self-check — which looks for itself
/// at `$HOME/.local/bin/claude` — finds a consistent install instead of warning "missing or
/// broken — run claude install to repair". The actually-exec'd binary stays the resolved
/// `claude_binary` path passed to the runner; these are just symlinks pointing at the same file.
#[cfg(unix)]
pub fn seed_claude_local_install(claude_home_dir: &Path, claude_binary: &str) -> Result<()> {
    use std::os::unix::fs::symlink;

    let real_bin = Path::new(claude_binary);
    let local_bin_dir = claude_home_dir.join(".local").join("bin");
    std::fs::create_dir_all(&local_bin_dir)
        .with_context(|| format!("create {}", local_bin_dir.display()))?;
    let local_bin_claude = local_bin_dir.join("claude");

    // Detect the installer's `.../versions/<version>/<real binary>` shape and mirror it so a
    // version-manifest check (if any) also finds a matching entry; otherwise fall back to a flat
    // symlink straight at the resolved binary.
    let link_target = if is_versioned_install_layout(real_bin) {
        mirror_versioned_symlink(claude_home_dir, real_bin)?
    } else {
        real_bin.to_path_buf()
    };

    let _ = std::fs::remove_file(&local_bin_claude);
    symlink(&link_target, &local_bin_claude).with_context(|| {
        format!(
            "symlink {} -> {}",
            local_bin_claude.display(),
            link_target.display()
        )
    })?;
    Ok(())
}

#[cfg(unix)]
fn is_versioned_install_layout(real_bin: &Path) -> bool {
    real_bin
        .parent()
        .and_then(|p| p.file_name())
        .is_some_and(|n| n == "versions")
}

/// Mirror `real_bin` (`.../versions/<version>/<binary>`) under
/// `claude_home_dir/.local/share/claude/versions/<version>`, returning the mirrored symlink path.
#[cfg(unix)]
fn mirror_versioned_symlink(claude_home_dir: &Path, real_bin: &Path) -> Result<PathBuf> {
    use std::os::unix::fs::symlink;

    let version = real_bin
        .file_name()
        .map(|n| n.to_owned())
        .context("versioned claude binary has no file name")?;
    let versions_dir = claude_home_dir
        .join(".local")
        .join("share")
        .join("claude")
        .join("versions");
    std::fs::create_dir_all(&versions_dir)
        .with_context(|| format!("create {}", versions_dir.display()))?;
    let versioned_link = versions_dir.join(&version);
    let _ = std::fs::remove_file(&versioned_link);
    symlink(real_bin, &versioned_link).with_context(|| {
        format!(
            "symlink {} -> {}",
            versioned_link.display(),
            real_bin.display()
        )
    })?;
    Ok(versioned_link)
}

/// Credentials copied into the jail HOME for Claude — `.credentials.json` only.
pub fn claude_credentials_copies(host_home: &Path, scratch_home: &Path) -> Vec<CopySpec> {
    vec![CopySpec {
        src: host_home.join(".claude").join(".credentials.json"),
        dest: scratch_home.join(".claude").join(".credentials.json"),
        optional: true,
        mode: Some(0o600),
    }]
}

/// Policy for an interactive Node/V8 CLI with PTY (Claude Code).
pub fn claude_interactive_policy() -> PolicySpec {
    PolicySpec {
        allow_dynamic_code_generation: true,
        allow_process_fork: true,
        mach_lookup: MachPolicy::All,
        sysctl_read: true,
        pseudo_tty: true,
        exec_paths: [
            "/usr/bin",
            "/usr/libexec",
            "/bin",
            "/sbin",
            "/System",
            "/Library",
            "/Applications",
            "/opt/homebrew",
            "/opt/local",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect(),
    }
}

/// Claude-specific env vars layered on [`tddy_sandbox::scratch_runner_env`].
pub fn claude_runner_env_overlay(scratch_tmp: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    let tmp = scratch_tmp.to_string_lossy().to_string();
    env.insert("CLAUDE_CODE_TMPDIR".into(), tmp.clone());
    env.insert("CLAUDE_TMPDIR".into(), tmp);
    env
}

const PERMISSION_PROMPT_TOOL: &str = "mcp__tddy-tools__approval_prompt";
const MCP_CONFIG_FILENAME: &str = "claude-mcp-config.json";

/// ACP-shaped subagent tools (see docs/ft/coder/managed-codebase-subagents.md) that a sandboxed
/// Claude must be allowed to call when a discovery subagent is wired into the session.
const SUBAGENT_TOOLS: &[&str] = &[
    "mcp__tddy-tools__subagent_new_session",
    "mcp__tddy-tools__subagent_prompt",
    "mcp__tddy-tools__subagent_cancel",
];

/// Session-action tools a session with a replaced `Shell` must be allowed to call instead
/// (see docs/ft/coder/no-bash-mode.md): request a new action from the Shell-replacing author
/// subagent, list established actions, invoke one.
const ACTION_TOOLS: &[&str] = &[
    "mcp__tddy-tools__request_action",
    "mcp__tddy-tools__list_actions",
    "mcp__tddy-tools__invoke_action",
];

/// Claude-native aliases of exec-catalog tools: replacing the exec tool must also hard-disable
/// these, or the agent falls back to the native built-in (`PermissionServer::decide` pre-allows
/// native calls on in-repo paths). The pairwise native+mcp disallow already covers same-named
/// built-ins (`Grep`, `Write`, …); this maps the *differently named* native routes.
fn native_aliases(exec_tool: &str) -> &'static [&'static str] {
    match exec_tool {
        "Shell" => &["Bash", "BashOutput", "KillShell"],
        "Write" => &["Edit", "MultiEdit", "NotebookEdit"],
        _ => &[],
    }
}

/// True when the session's replaced-tool set removes direct shell — the trigger for the
/// session-action surface ([`ACTION_TOOLS`]).
pub fn shell_is_replaced(replaced: &[&str]) -> bool {
    replaced.contains(&"Shell")
}

/// `--allowedTools` entries for sandbox claude: `mcp__tddy-tools__*` exec tools (minus any tool
/// named in `replaced` — a subagent that declares it replaces that tool means a direct call must
/// be impossible, not merely discouraged) + `AskUserQuestion`, plus the subagent tools when
/// `subagent_enabled` is `true`, plus the session-action tools when `Shell` itself is replaced
/// (with no direct shell, established session actions are the only way to run a command).
pub fn build_claude_allowlist(subagent_enabled: bool, replaced: &[&str]) -> Vec<String> {
    let remaining_tools: Vec<&str> = workspace_exec_tool_names()
        .iter()
        .filter(|name| !replaced.contains(name))
        .copied()
        .collect();
    let mut allowlist =
        tddy_workflow_recipes::permissions::build_remote_allowlist(&remaining_tools);
    if subagent_enabled {
        allowlist.extend(SUBAGENT_TOOLS.iter().map(|s| s.to_string()));
    }
    if shell_is_replaced(replaced) {
        allowlist.extend(ACTION_TOOLS.iter().map(|s| s.to_string()));
    }
    allowlist
}

/// `--disallowedTools` entries for sandbox claude: each tool a wired-in subagent `replaced` must
/// be *unreachable*, not merely absent from the allowlist. Dropping it from `--allowedTools` only
/// un-pre-approves it — Claude's native built-in (`Grep`/`Glob`/…) and the still-advertised
/// `mcp__tddy-tools__*` form stay callable via the permission prompt. `--disallowedTools` takes
/// precedence and removes them outright, so the only route to a replaced tool is the subagent (or,
/// for `Shell`, the session-action surface). Both forms are listed; a name with no native Claude
/// built-in (e.g. `SemanticSearch`) simply has no native counterpart to match, which is harmless.
/// Differently-named native aliases ([`native_aliases`]: `Bash*` for `Shell`, `Edit`/`MultiEdit`/
/// `NotebookEdit` for `Write`) are appended so a replacement covers every native route. Empty when
/// nothing is replaced.
pub fn build_claude_disallowlist(replaced: &[&str]) -> Vec<String> {
    let mut disallowed = Vec::with_capacity(replaced.len() * 2);
    for tool in replaced {
        disallowed.push((*tool).to_string());
        disallowed.push(format!("mcp__tddy-tools__{tool}"));
    }
    for tool in replaced {
        for alias in native_aliases(tool) {
            if !disallowed.iter().any(|t| t == alias) {
                disallowed.push((*alias).to_string());
            }
        }
    }
    disallowed
}

/// Write MCP config registering `tddy-tools --mcp` under a writable scratch directory. `mcp_env`,
/// when non-empty, becomes the server's `env` block — the sandbox runner uses it to set
/// `TDDY_TOOLS_LOG_FILE` (persist the in-jail MCP server's logs) and `RUST_LOG` so the process
/// Claude spawns is observable.
pub fn write_claude_mcp_config(
    scratch_dir: &Path,
    tddy_tools_path: &Path,
    mcp_env: &BTreeMap<String, String>,
) -> Result<PathBuf> {
    std::fs::create_dir_all(scratch_dir).with_context(|| {
        format!(
            "create scratch dir for MCP config: {}",
            scratch_dir.display()
        )
    })?;
    let path = scratch_dir.join(MCP_CONFIG_FILENAME);
    let mut server = serde_json::json!({
        "command": tddy_tools_path.to_string_lossy(),
        "args": ["--mcp"]
    });
    if !mcp_env.is_empty() {
        server["env"] = serde_json::json!(mcp_env);
    }
    let config = serde_json::json!({ "mcpServers": { "tddy-tools": server } });
    std::fs::write(&path, config.to_string())
        .with_context(|| format!("write MCP config: {}", path.display()))?;
    Ok(path)
}

/// Append `--allowedTools`, `--permission-prompt-tool`, and `--mcp-config` for sandbox spawn.
/// `subagent_enabled` mirrors `tddy_tools::server::subagent_enabled()`'s `TDDY_SUBAGENT` check —
/// the caller decides, so this crate stays free of an env-reading dependency of its own. `replaced`
/// is the set of exec tools the wired-in subagents declare they replace (see
/// [`build_claude_allowlist`]).
pub fn append_claude_mcp_args(
    argv: &mut Vec<String>,
    scratch_dir: &Path,
    tddy_tools_path: &Path,
    subagent_enabled: bool,
    replaced: &[&str],
    mcp_env: &BTreeMap<String, String>,
) -> Result<()> {
    let mcp_path = write_claude_mcp_config(scratch_dir, tddy_tools_path, mcp_env)?;
    for tool in build_claude_allowlist(subagent_enabled, replaced) {
        argv.push("--allowedTools".into());
        argv.push(tool);
    }
    for tool in build_claude_disallowlist(replaced) {
        argv.push("--disallowedTools".into());
        argv.push(tool);
    }
    argv.push("--permission-prompt-tool".into());
    argv.push(PERMISSION_PROMPT_TOOL.into());
    argv.push("--mcp-config".into());
    argv.push(mcp_path.to_string_lossy().into_owned());
    Ok(())
}

/// Writable directory for sandbox MCP config (context dir is read-only).
pub fn claude_scratch_mcp_dir(fallback: &Path) -> PathBuf {
    std::env::var("TMPDIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    // Only the macOS-gated dyld-root test below consumes these; gate the import to match so it is
    // not flagged unused on Linux.
    #[cfg(target_os = "macos")]
    use tddy_sandbox::builder::{ReadKind, ReadReason};

    const NATIVE_TOOLS_EXCLUDED: &[&str] = &[
        "Read",
        "Write",
        "Edit",
        "Glob",
        "Grep",
        "Bash",
        "Shell",
        "SemanticSearch",
    ];

    #[test]
    fn claude_allowlist_uses_mcp_prefix_and_excludes_native_tools() {
        let allowlist = build_claude_allowlist(false, &[]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for name in workspace_exec_tool_names() {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "allowlist must contain {prefixed}; got: {allowlist:?}"
            );
        }
        assert!(allowset.contains("AskUserQuestion"));
        for native in NATIVE_TOOLS_EXCLUDED {
            assert!(
                !allowset.contains(*native),
                "native tool {native} must not appear un-prefixed in sandbox allowlist"
            );
        }
    }

    /// Feature: docs/ft/coder/managed-codebase-subagents.md (criterion 10)
    /// Changeset: docs/dev/1-WIP/2026-07-01-changeset-managed-codebase-subagents.md
    ///
    /// When a discovery subagent is wired into the session, the sandboxed Claude CLI's
    /// `--allowedTools` must include the three ACP-shaped subagent tools — otherwise Claude Code
    /// would have no way to call `subagent_new_session`/`subagent_prompt`/`subagent_cancel` even
    /// though the MCP server exposes them.
    #[test]
    fn claude_allowlist_includes_subagent_tools_when_subagent_is_enabled() {
        let allowlist = build_claude_allowlist(true, &[]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for tool in [
            "mcp__tddy-tools__subagent_new_session",
            "mcp__tddy-tools__subagent_prompt",
            "mcp__tddy-tools__subagent_cancel",
        ] {
            assert!(
                allowset.contains(tool),
                "allowlist must contain {tool} when a subagent is enabled; got: {allowlist:?}"
            );
        }
    }

    /// The converse of the above: without a subagent wired in, none of the three subagent tools
    /// should be advertised to Claude — there is nothing behind them to dispatch to.
    #[test]
    fn claude_allowlist_omits_subagent_tools_when_subagent_is_disabled() {
        let allowlist = build_claude_allowlist(false, &[]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for tool in [
            "mcp__tddy-tools__subagent_new_session",
            "mcp__tddy-tools__subagent_prompt",
            "mcp__tddy-tools__subagent_cancel",
        ] {
            assert!(
                !allowset.contains(tool),
                "allowlist must NOT contain {tool} when no subagent is enabled; got: {allowlist:?}"
            );
        }
    }

    /// Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criterion 15)
    /// Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md
    ///
    /// A subagent that declares it replaces `Grep`/`Glob` must remove those two tools from the
    /// allowlist — a direct call to either must be impossible, not merely discouraged. Every
    /// other exec tool, `AskUserQuestion`, and the subagent tools themselves stay present.
    #[test]
    fn claude_allowlist_omits_replaced_tools_but_keeps_everything_else() {
        let allowlist = build_claude_allowlist(true, &["Grep", "Glob"]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        assert!(
            !allowset.contains("mcp__tddy-tools__Grep"),
            "replaced tool Grep must not appear in the allowlist: {allowlist:?}"
        );
        assert!(
            !allowset.contains("mcp__tddy-tools__Glob"),
            "replaced tool Glob must not appear in the allowlist: {allowlist:?}"
        );
        for name in workspace_exec_tool_names()
            .iter()
            .filter(|n| **n != "Grep" && **n != "Glob")
        {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "non-replaced tool {prefixed} must stay in the allowlist: {allowlist:?}"
            );
        }
        for tool in [
            "AskUserQuestion",
            "mcp__tddy-tools__subagent_new_session",
            "mcp__tddy-tools__subagent_prompt",
            "mcp__tddy-tools__subagent_cancel",
        ] {
            assert!(
                allowset.contains(tool),
                "{tool} must still be present alongside a replaced-tool filter: {allowlist:?}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn claude_exec_reads_include_the_dyld_root_literal() {
        let reads = process_claude_exec_reads(Path::new("/usr/bin/true"));
        assert!(
            reads.iter().any(|r| {
                r.kind == ReadKind::Literal
                    && r.host == Path::new("/")
                    && r.reason == ReadReason::DyldRoot
            }),
            "claude reads must include the dyld root literal: {reads:?}"
        );
    }

    /// `seed_claude_credentials` never overwrites an existing dest — a persistent jail that has
    /// refreshed its own OAuth token must not be clobbered by the (possibly stale) host copy on a
    /// later restart. This no-clobber guarantee is what makes auth *persist* across sessions.
    #[test]
    fn seed_claude_credentials_does_not_overwrite_an_existing_token() {
        let home = tempfile::tempdir().unwrap();
        let dest = home.path().join(".claude").join(".credentials.json");
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, "JAIL-REFRESHED-TOKEN").unwrap();

        seed_claude_credentials(home.path()).expect("seed must succeed");

        assert_eq!(
            std::fs::read_to_string(&dest).unwrap(),
            "JAIL-REFRESHED-TOKEN",
            "an existing jail token must survive seeding"
        );
    }

    #[test]
    fn claude_credentials_copies_seed_only_the_credentials_file() {
        let copies = claude_credentials_copies(Path::new("/home/user"), Path::new("/jail/home"));
        assert_eq!(
            copies.len(),
            1,
            "expected only the credentials copy: {copies:?}"
        );
        assert!(
            copies[0].src.ends_with(".claude/.credentials.json"),
            "copy src must be the credentials file: {:?}",
            copies[0].src
        );
        assert!(
            copies
                .iter()
                .all(|c| !c.src.to_string_lossy().contains("settings")),
            "settings.json must not be seeded into the jail: {copies:?}"
        );
    }

    #[test]
    fn write_claude_mcp_config_registers_tddy_tools_mcp_server() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let path = write_claude_mcp_config(dir.path(), &tools, &BTreeMap::new())
            .expect("write MCP config must succeed");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(server["command"].as_str().unwrap(), tools.to_string_lossy());
        assert_eq!(server["args"], serde_json::json!(["--mcp"]));
        assert!(
            server.get("env").is_none(),
            "no env block when mcp_env is empty"
        );
    }

    /// A non-empty `mcp_env` is written as the server's `env` block so the in-jail `--mcp` process
    /// (e.g. its log file + RUST_LOG) is configured by the runner.
    #[test]
    fn write_claude_mcp_config_includes_env_block_when_given() {
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let mut env = BTreeMap::new();
        env.insert(
            "TDDY_TOOLS_LOG_FILE".to_string(),
            "/egress/tddy-tools.mcp.log".to_string(),
        );
        env.insert("RUST_LOG".to_string(), "info".to_string());
        let path = write_claude_mcp_config(dir.path(), &tools, &env)
            .expect("write MCP config must succeed");
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let server = &json["mcpServers"]["tddy-tools"];
        assert_eq!(
            server["env"]["TDDY_TOOLS_LOG_FILE"].as_str().unwrap(),
            "/egress/tddy-tools.mcp.log"
        );
        assert_eq!(server["env"]["RUST_LOG"].as_str().unwrap(), "info");
    }

    // ─── replaced tools must be hard-disabled, not merely un-allowlisted ─────────────
    //
    // Removing a replaced tool from `--allowedTools` only un-pre-approves it; Claude's native
    // built-in Grep/Glob and the still-advertised `mcp__tddy-tools__*` form remain reachable via
    // the permission prompt. A subagent that replaces a tool means a direct call must be
    // impossible — the only route is delegating to the subagent — so those names must be passed to
    // Claude's `--disallowedTools`, which takes precedence and removes them from availability.

    /// The tool names Claude is told to hard-disable: the value following each `--disallowedTools`
    /// token in the built argv.
    fn disallowed_tools_in(argv: &[String]) -> HashSet<String> {
        argv.windows(2)
            .filter(|w| w[0] == "--disallowedTools")
            .map(|w| w[1].clone())
            .collect()
    }

    /// A subagent replacing `Grep`/`Glob` must hard-disable Claude's native built-in Grep/Glob, so
    /// the agent cannot fall back to them directly instead of delegating to the subagent.
    /// (`SemanticSearch` has no native Claude built-in — only the MCP form, covered below.)
    #[test]
    fn append_claude_mcp_args_hard_disables_the_native_form_of_replaced_tools() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let mut argv = vec!["claude".to_string()];

        // When
        append_claude_mcp_args(
            &mut argv,
            dir.path(),
            &tools,
            true,
            &["Grep", "Glob", "SemanticSearch"],
            &BTreeMap::new(),
        )
        .expect("append must succeed");

        // Then
        let disallowed = disallowed_tools_in(&argv);
        assert!(
            disallowed.contains("Grep"),
            "native Grep must be hard-disabled; disallowed set: {disallowed:?}"
        );
        assert!(
            disallowed.contains("Glob"),
            "native Glob must be hard-disabled; disallowed set: {disallowed:?}"
        );
    }

    /// The still-advertised MCP form of every replaced tool must also be hard-disabled — otherwise
    /// the model could call `mcp__tddy-tools__Grep`/`…SemanticSearch` directly even though they are
    /// absent from the allowlist.
    #[test]
    fn append_claude_mcp_args_hard_disables_the_mcp_form_of_replaced_tools() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let mut argv = vec!["claude".to_string()];

        // When
        append_claude_mcp_args(
            &mut argv,
            dir.path(),
            &tools,
            true,
            &["Grep", "Glob", "SemanticSearch"],
            &BTreeMap::new(),
        )
        .expect("append must succeed");

        // Then
        let disallowed = disallowed_tools_in(&argv);
        for tool in [
            "mcp__tddy-tools__Grep",
            "mcp__tddy-tools__Glob",
            "mcp__tddy-tools__SemanticSearch",
        ] {
            assert!(
                disallowed.contains(tool),
                "MCP form {tool} must be hard-disabled; disallowed set: {disallowed:?}"
            );
        }
    }

    // ─── tool replacement: Shell / write tools (no-bash & no-write via `replaces`) ───
    //
    // Feature: docs/ft/coder/no-bash-mode.md
    // Everything is driven by the defs' `replaces` sets: replacing `Shell` swaps in the
    // session-action surface and hard-disables the native Bash family; replacing `Write` also
    // hard-disables the native edit aliases. No dedicated mode flags exist.

    /// Replacing `Shell` drops it from the allowlist, adds the three session-action tools, and
    /// keeps every other exec tool (including the write tools).
    #[test]
    fn replacing_shell_swaps_it_for_the_action_tools() {
        let allowlist = build_claude_allowlist(true, &["Shell"]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        assert!(
            !allowset.contains("mcp__tddy-tools__Shell"),
            "Shell must leave the allowlist when replaced: {allowlist:?}"
        );
        for tool in ACTION_TOOLS {
            assert!(
                allowset.contains(*tool),
                "a Shell replacement must advertise {tool}: {allowlist:?}"
            );
        }
        for name in workspace_exec_tool_names().iter().filter(|n| **n != "Shell") {
            let prefixed = format!("mcp__tddy-tools__{name}");
            assert!(
                allowset.contains(&prefixed),
                "non-shell tool {prefixed} must stay: {allowlist:?}"
            );
        }
    }

    /// Replacing `Shell` hard-disables both its forms AND Claude's native Bash family — the
    /// differently-named native aliases the pairwise disallow can't cover.
    #[test]
    fn replacing_shell_hard_disables_the_native_bash_family() {
        let disallowed: HashSet<_> = build_claude_disallowlist(&["Shell"]).into_iter().collect();
        for tool in [
            "Shell",
            "mcp__tddy-tools__Shell",
            "Bash",
            "BashOutput",
            "KillShell",
        ] {
            assert!(
                disallowed.contains(tool),
                "a Shell replacement must hard-disable {tool}: {disallowed:?}"
            );
        }
    }

    /// Replacing the write tools drops them from the allowlist while `Shell` and the read tools
    /// stay, and the action tools are NOT advertised (they belong to a Shell replacement).
    #[test]
    fn replacing_the_write_tools_keeps_shell_and_adds_no_action_tools() {
        let allowlist = build_claude_allowlist(true, &["Write", "StrReplace", "Delete"]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();

        for tool in [
            "mcp__tddy-tools__Write",
            "mcp__tddy-tools__StrReplace",
            "mcp__tddy-tools__Delete",
        ] {
            assert!(
                !allowset.contains(tool),
                "replaced mutation tool {tool} must leave the allowlist: {allowlist:?}"
            );
        }
        for tool in [
            "mcp__tddy-tools__Shell",
            "mcp__tddy-tools__Read",
            "mcp__tddy-tools__Grep",
        ] {
            assert!(
                allowset.contains(tool),
                "{tool} must stay: {allowlist:?}"
            );
        }
        for tool in ACTION_TOOLS {
            assert!(
                !allowset.contains(*tool),
                "a write-only replacement must not advertise {tool}: {allowlist:?}"
            );
        }
    }

    /// Replacing `Write` hard-disables Claude's native edit aliases (`Edit`/`MultiEdit`/
    /// `NotebookEdit`) alongside the pairwise forms — otherwise the agent falls back to the
    /// native built-ins, which the permission server pre-allows on in-repo paths.
    #[test]
    fn replacing_write_hard_disables_the_native_edit_aliases() {
        let disallowed: HashSet<_> =
            build_claude_disallowlist(&["Write", "StrReplace", "Delete"])
                .into_iter()
                .collect();
        for tool in [
            "Write",
            "mcp__tddy-tools__Write",
            "StrReplace",
            "Delete",
            "Edit",
            "MultiEdit",
            "NotebookEdit",
        ] {
            assert!(
                disallowed.contains(tool),
                "a Write replacement must hard-disable {tool}: {disallowed:?}"
            );
        }
    }

    /// Shell and write replacements compose (e.g. one gemma action-author plus one coder def)
    /// into a deduplicated union.
    #[test]
    fn shell_and_write_replacements_compose_without_duplicates() {
        let disallowed =
            build_claude_disallowlist(&["Shell", "Write", "StrReplace", "Delete"]);
        let disallowset: HashSet<_> = disallowed.iter().cloned().collect();
        assert_eq!(
            disallowed.len(),
            disallowset.len(),
            "composed disallowlist must not contain duplicates: {disallowed:?}"
        );
        for tool in ["Shell", "Bash", "Write", "Edit", "mcp__tddy-tools__Delete"] {
            assert!(disallowset.contains(tool), "missing {tool}: {disallowed:?}");
        }

        let allowlist =
            build_claude_allowlist(true, &["Shell", "Write", "StrReplace", "Delete"]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();
        for tool in ACTION_TOOLS {
            assert!(
                allowset.contains(*tool),
                "the composed session must still advertise {tool}: {allowlist:?}"
            );
        }
    }

    /// Regression guard: with no replacements the lists are exactly the pre-existing ones —
    /// empty disallow, full exec allowlist, no action tools.
    #[test]
    fn no_replacements_changes_nothing() {
        assert!(build_claude_disallowlist(&[]).is_empty());
        let allowlist = build_claude_allowlist(true, &[]);
        let allowset: HashSet<_> = allowlist.iter().cloned().collect();
        for name in workspace_exec_tool_names() {
            assert!(allowset.contains(&format!("mcp__tddy-tools__{name}")));
        }
        for tool in ACTION_TOOLS {
            assert!(
                !allowset.contains(*tool),
                "no replacement must not advertise {tool}"
            );
        }
    }

    /// With no replaced tools, nothing is hard-disabled — the disallow list must not gratuitously
    /// remove tools the agent legitimately uses.
    #[test]
    fn append_claude_mcp_args_disables_nothing_when_no_tools_are_replaced() {
        // Given
        let dir = tempfile::tempdir().unwrap();
        let tools = dir.path().join("tddy-tools");
        let mut argv = vec!["claude".to_string()];

        // When
        append_claude_mcp_args(&mut argv, dir.path(), &tools, true, &[], &BTreeMap::new())
            .expect("append must succeed");

        // Then
        assert!(
            disallowed_tools_in(&argv).is_empty(),
            "nothing must be disallowed when nothing is replaced: {argv:?}"
        );
    }
}

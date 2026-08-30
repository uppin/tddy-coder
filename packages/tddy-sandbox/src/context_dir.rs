//! The directory an agent gets as its working directory when the codebase is managed.
//!
//! It looks like a repository and is not one: it holds only what the session's backend reads for
//! guidance, copied out of the *target* repo under that backend's glob allow-list. The one thing
//! standing between an agent and that mismatch is the preamble, which is why it leads
//! `CLAUDE.md`/`AGENTS.md` rather than trailing them — the rule that the codebase is elsewhere has
//! to be read before the project's own thousands of words, not after.
//!
//! PRD: docs/ft/daemon/agent-context-sync.md

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::context_manifest::walk_context_files;

/// Prepended to CLAUDE.md and AGENTS.md in the context dir.
pub const MANAGED_CODEBASE_PREAMBLE: &str = r#"## Managed Codebase

The real codebase is MANAGED — it is NOT in this local directory.
This directory holds only a synced copy of that codebase's agent configuration.

You MUST use the `mcp__tddy-tools__*` tools (Read, Write, StrReplace, Delete, Grep, Glob, Shell,
Await, ReadLints, SemanticSearch) for ALL file and shell operations.
Do not use native tools to interact with the codebase.

This directory is owned by the sync: `CLAUDE.md`, `AGENTS.md` and the agent-configuration trees
beside them are re-read from the managed codebase while the session runs, and anything you write
under those paths is replaced. Keep your work in the codebase itself, through the tools above.
"#;

/// The files an agent reads for guidance before anything else, and so the ones the preamble has to
/// lead. Both are written even when the target repo has neither — a repo with no `CLAUDE.md` would
/// otherwise leave the agent with nothing telling it where its codebase is.
///
/// Exported, and the *only* spelling of this list anywhere: the co-located builder here, the split
/// builder (`tddy_daemon::split_session::build_split_context_dir`) and the re-sync writer
/// (`tddy_daemon::context_sync`) all have to agree on which files carry the preamble, and they used
/// to agree by each keeping their own copy. That is how a re-sync tick came to be able to delete a
/// `CLAUDE.md` that both builders guarantee exists. A list every conventional filename has to be
/// added to — `.cursorrules`, `GEMINI.md` — cannot be kept in step by remembering to.
pub const PREAMBLE_FILES: &[&str] = &["CLAUDE.md", "AGENTS.md"];

/// Carried at the head of the preamble while the last re-sync is known to have failed.
///
/// Deliberately wordier than the bare word "stale": [`without_stale_marker`] strips it by exact
/// text, so a project whose own `CLAUDE.md` discusses stale caches must not have its content
/// mangled when the line is cleared.
const CONTEXT_STALE_MARKER: &str = "> **THIS CONTEXT IS STALE** — the last re-sync from the managed codebase failed, so the guidance below may no longer match it.";

/// One agent and the exec-catalog tools it handles instead of the main agent, for rendering a
/// per-agent breakdown in the managed-codebase preamble.
pub struct SubagentReplacement<'a> {
    pub name: &'a str,
    pub replaced: &'a [&'a str],
}

/// Managed-codebase preamble, optionally naming one or more subagents that each replace some of
/// the listed tools. With an empty `replacements` slice (or every entry's `replaced` empty), this
/// is exactly [`MANAGED_CODEBASE_PREAMBLE`]. Otherwise one enforcement paragraph is appended naming
/// each replacing agent next to its own replaced tools.
///
/// A replaced `Shell` gets its own paragraph instead of the generic subagent-delegation hint:
/// commands then run only through declarative **session actions** (`request_action` — authored
/// by the Shell-replacing agent — then `invoke_action`; see docs/ft/coder/no-bash-mode.md).
pub fn managed_codebase_preamble(replacements: &[SubagentReplacement<'_>]) -> String {
    let mut preamble = MANAGED_CODEBASE_PREAMBLE.to_string();

    let shell_author = replacements
        .iter()
        .find(|r| r.replaced.contains(&"Shell"))
        .map(|r| r.name);

    // Generic delegation clauses cover every replaced tool except Shell, whose replacement is
    // surfaced through the session-action tools rather than subagent_prompt.
    let clauses: Vec<String> = replacements
        .iter()
        .filter_map(|r| {
            let delegated: Vec<&str> = r
                .replaced
                .iter()
                .filter(|t| **t != "Shell")
                .copied()
                .collect();
            if delegated.is_empty() {
                return None;
            }
            Some(format!(
                "{} — handled by the `{}` subagent",
                delegated.join(", "),
                r.name
            ))
        })
        .collect();
    if !clauses.is_empty() {
        let agent_hint = if clauses.len() > 1 {
            " (pass `agent: \"<name>\"` to select which subagent)"
        } else {
            ""
        };
        preamble.push_str(&format!(
            "\n\
The following tools are NOT available as direct tools — they are handled by specialized \
subagents instead: {}.\n\
Use `mcp__tddy-tools__subagent_new_session`{} and `mcp__tddy-tools__subagent_prompt` to perform \
those operations.\n",
            clauses.join("; "),
            agent_hint
        ));
    }

    if let Some(author) = shell_author {
        preamble.push_str(&format!(
            "\n\
Shell is NOT available in this session. Commands run only through declarative session actions:\n\
1. `mcp__tddy-tools__request_action` — describe the command you need in natural language; the \
`{author}` agent writes a bounded action manifest for it.\n\
2. `mcp__tddy-tools__list_actions` — list the actions already established.\n\
3. `mcp__tddy-tools__invoke_action` — run an established action by id.\n\
Prefer reusing an established action over requesting a near-duplicate; request narrow, \
single-purpose actions (e.g. one per test suite) rather than broad wrappers.\n"
        ));
    }
    preamble
}

/// `body` with `preamble` in front of it, separated by a blank line so the preamble's last line and
/// the project's first heading do not run together into one markdown block.
///
/// Composing an already-composed file is a no-op. Re-sync rewrites `CLAUDE.md` from the repo's
/// bytes every time it changes, so a caller that fed this its own previous output would otherwise
/// double the notice on every tick.
pub fn prepend_preamble(preamble: &str, body: &str) -> String {
    let preamble = preamble.trim();
    if body.trim_start().starts_with(preamble) {
        return body.to_string();
    }
    format!("{preamble}\n\n{body}")
}

/// `content` carrying the staleness line, or `content` unchanged if it already carries it.
///
/// The line goes at the head, above the preamble it qualifies: a link that is down for ten ticks
/// must warn once, not bury the guidance under ten identical lines.
pub fn with_stale_marker(content: &str) -> String {
    if content.contains(CONTEXT_STALE_MARKER) {
        return content.to_string();
    }
    format!("{CONTEXT_STALE_MARKER}\n\n{content}")
}

/// `content` with the staleness line removed, byte-identical to what it was marked from.
///
/// A no-op on content that was never marked, so a syncer can call it on every success without
/// reading the file first.
pub fn without_stale_marker(content: &str) -> String {
    content.replace(&format!("{CONTEXT_STALE_MARKER}\n\n"), "")
}

/// Tells the agent, where it reads, that the guidance may have drifted.
///
/// A re-sync that fails leaves the session running — dropping a working session over a transient
/// link failure would be worse than the staleness — so the warning is how the agent learns not to
/// trust what it is reading. Files the context dir does not have are left alone: the marker belongs
/// to the guidance that is there, not to a file the backend never asked for.
pub fn mark_context_stale(context_dir: &Path) -> anyhow::Result<()> {
    rewrite_preamble_files(context_dir, with_stale_marker)
}

/// Withdraws the staleness line after a re-sync succeeds.
pub fn clear_context_stale(context_dir: &Path) -> anyhow::Result<()> {
    rewrite_preamble_files(context_dir, without_stale_marker)
}

fn rewrite_preamble_files(
    context_dir: &Path,
    rewrite: impl Fn(&str) -> String,
) -> anyhow::Result<()> {
    for filename in PREAMBLE_FILES {
        let path = context_dir.join(filename);
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)?;
        let rewritten = rewrite(&content);
        if rewritten != content {
            std::fs::write(&path, rewritten)?;
        }
    }
    Ok(())
}

/// RAII wrapper for a context directory used inside the sandbox.
pub struct SandboxContextDir {
    dir: tempfile::TempDir,
}

impl SandboxContextDir {
    /// Creates a temp context dir holding everything in `source_dir` that `globs` names, with the
    /// managed-codebase preamble leading `CLAUDE.md` and `AGENTS.md`.
    ///
    /// `globs` is the session backend's allow-list, so a Cursor session gets `.cursor/` and a
    /// Claude one does not. Nothing else in the repository is copied.
    pub fn create(source_dir: &Path, globs: &[&str]) -> anyhow::Result<Self> {
        Self::create_with_subagent(source_dir, &[], globs)
    }

    /// Like [`Self::create`], but the preamble names each entry in `replacements` next to the exec
    /// tools it replaces for this session (see [`managed_codebase_preamble`]).
    pub fn create_with_subagent(
        source_dir: &Path,
        replacements: &[SubagentReplacement<'_>],
        globs: &[&str],
    ) -> anyhow::Result<Self> {
        let dir = tempfile::tempdir()?;
        copy_context_from_repo(source_dir, dir.path(), globs)?;

        let preamble = managed_codebase_preamble(replacements);
        for filename in PREAMBLE_FILES {
            let dest = dir.path().join(filename);
            let body = if dest.exists() {
                std::fs::read_to_string(&dest)?
            } else {
                String::new()
            };
            std::fs::write(&dest, prepend_preamble(&preamble, &body))?;
        }

        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Copy everything `globs` names out of a repo/worktree into a context directory.
///
/// The result stays writable. Re-sync writes into the live directory for as long as the session
/// runs, and freezing it at 0444 would buy nothing anyway: the agent is held out by the jail's
/// read-only mount and by the native-tool disallowlist, neither of which reads the file mode.
pub fn copy_context_from_repo(source: &Path, dest: &Path, globs: &[&str]) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest)?;
    for file in walk_context_files(source, globs)? {
        let target = dest.join(&file.rel_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&file.source, &target)?;
        ensure_owner_writable(&target)?;
    }
    Ok(())
}

/// `fs::copy` carries the source mode across, so a target repo that keeps its `CLAUDE.md` read-only
/// would hand the syncer a copy it cannot update on the next tick.
#[cfg(unix)]
fn ensure_owner_writable(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)?.permissions();
    let mode = permissions.mode();
    if mode & 0o200 == 0 {
        permissions.set_mode(mode | 0o200);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_owner_writable(path: &Path) -> anyhow::Result<()> {
    let mut permissions = std::fs::metadata(path)?.permissions();
    if permissions.readonly() {
        permissions.set_readonly(false);
        std::fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

/// Recursively copy a file or directory tree, following symlinks with cycle detection.
pub fn copy_tree(src: &Path, dst: &Path) -> anyhow::Result<()> {
    copy_tree_inner(src, dst, None, &mut HashSet::new())
}

/// Like [`copy_tree`] but skips symlinks that resolve outside `root` (prevents copying `node_modules` via `.claude` links).
pub fn copy_tree_within_root(
    src: &Path,
    dst: &Path,
    root: &Path,
    visited: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let root = std::fs::canonicalize(root)?;
    copy_tree_inner(src, dst, Some(&root), visited)
}

fn copy_tree_inner(
    src: &Path,
    dst: &Path,
    root: Option<&Path>,
    visited: &mut HashSet<PathBuf>,
) -> anyhow::Result<()> {
    let meta = std::fs::symlink_metadata(src)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(src)?;
        let resolved = if target.is_absolute() {
            target
        } else {
            src.parent()
                .ok_or_else(|| anyhow::anyhow!("symlink without parent: {}", src.display()))?
                .join(target)
        };
        let canonical = std::fs::canonicalize(&resolved).unwrap_or(resolved);
        if let Some(root) = root {
            if !canonical.starts_with(root) {
                return Ok(());
            }
        }
        if !visited.insert(canonical.clone()) {
            return Ok(());
        }
        return copy_tree_inner(&canonical, dst, root, visited);
    }
    if meta.is_dir() {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_tree_inner(&entry.path(), &dst.join(entry.file_name()), root, visited)?;
        }
        Ok(())
    } else if meta.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        Ok(())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::SandboxSpec;

    /// The allow-list a Claude session syncs, spelled out here so these tests stay independent of
    /// tddy-core's per-backend table.
    const CLAUDE_GLOBS: &[&str] = &[
        "CLAUDE.md",
        "AGENTS.md",
        ".claude/**",
        ".mcp.json",
        ".agents/**",
    ];

    #[test]
    fn the_context_dirs_claude_md_leads_with_the_preamble_and_keeps_the_repos_own_text() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(
            source_dir.path().join("CLAUDE.md"),
            "# CLAUDE.md\n\nOriginal instructions.\n",
        )
        .unwrap();

        // When
        let ctx = SandboxContextDir::create(source_dir.path(), CLAUDE_GLOBS)
            .expect("create must succeed");
        let claude_md =
            std::fs::read_to_string(ctx.path().join("CLAUDE.md")).expect("CLAUDE.md must exist");

        // Then
        assert!(claude_md.starts_with(MANAGED_CODEBASE_PREAMBLE.trim_start()));
        assert!(claude_md.contains("Original instructions."));
        assert!(claude_md.contains("mcp__tddy-tools__"));
    }

    // ─── managed_codebase_preamble / create_with_subagent ───────────────────────
    //
    // Feature: docs/ft/coder/managed-codebase-subagents.md § Tool replacement (criterion 16)
    // Changeset: docs/dev/1-WIP/2026-07-02-changeset-subagent-tool-replacement.md

    /// With no replaced tools, the rendered preamble is the constant itself — no enforcement
    /// paragraph, and every exec tool (including Grep/Glob) is still advertised as available.
    #[test]
    fn preamble_with_no_replaced_tools_is_the_static_constant() {
        // When
        let rendered = managed_codebase_preamble(&[]);

        // Then
        assert_eq!(rendered, MANAGED_CODEBASE_PREAMBLE);
    }

    /// A single agent's replaced set names it and the specific tools it replaces, and states
    /// those tools are not available directly.
    #[test]
    fn preamble_single_agent_names_the_agent_and_its_tools() {
        // When
        let rendered = managed_codebase_preamble(&[SubagentReplacement {
            name: "explorer",
            replaced: &["Grep", "Glob"],
        }]);

        // Then
        assert!(
            rendered.contains("explorer"),
            "preamble must name the replacing subagent: {rendered}"
        );
        assert!(
            rendered.contains("Grep") && rendered.contains("Glob"),
            "preamble must name the replaced tools: {rendered}"
        );
        assert!(
            rendered.contains("not available") || rendered.contains("NOT available"),
            "preamble must state the replaced tools are unavailable directly: {rendered}"
        );
    }

    /// With two agents each replacing different tools, the preamble names each agent next to its
    /// own tools — not a flat, unattributed union.
    #[test]
    fn preamble_renders_per_agent_breakdown_for_multiple_agents() {
        // When
        let rendered = managed_codebase_preamble(&[
            SubagentReplacement {
                name: "explorer",
                replaced: &["Grep", "Glob"],
            },
            SubagentReplacement {
                name: "my-linter",
                replaced: &["ReadLints"],
            },
        ]);

        // Then
        assert!(
            rendered.contains("explorer") && rendered.contains("my-linter"),
            "preamble must name both agents: {rendered}"
        );
        assert!(
            rendered.contains("Grep")
                && rendered.contains("Glob")
                && rendered.contains("ReadLints"),
            "preamble must name every replaced tool: {rendered}"
        );
        assert!(
            rendered.contains("agent:"),
            "preamble must hint how to address a specific agent when more than one replaces \
             something: {rendered}"
        );
    }

    /// A replaced `Shell` renders the session-action guidance (request_action → invoke_action,
    /// naming the authoring agent) instead of the generic subagent-delegation hint — the actions
    /// surface, not `subagent_prompt`, is how commands run in that session.
    /// Feature: docs/ft/coder/no-bash-mode.md
    #[test]
    fn preamble_renders_the_session_action_surface_for_a_replaced_shell() {
        // When
        let rendered = managed_codebase_preamble(&[SubagentReplacement {
            name: "action-author",
            replaced: &["Shell"],
        }]);

        // Then — the actions surface is described and attributed to the author
        for needle in [
            "request_action",
            "list_actions",
            "invoke_action",
            "action-author",
        ] {
            assert!(
                rendered.contains(needle),
                "preamble must mention {needle}: {rendered}"
            );
        }
        // And — Shell is not presented as subagent-delegated prompt work
        assert!(
            !rendered.contains("Shell — handled by"),
            "a replaced Shell must not use the generic delegation clause: {rendered}"
        );
    }

    /// An agent replacing Shell *and* other tools splits cleanly: the other tools get the
    /// generic delegation clause, Shell gets the session-action paragraph.
    #[test]
    fn preamble_splits_shell_from_an_agents_other_replacements() {
        // When
        let rendered = managed_codebase_preamble(&[SubagentReplacement {
            name: "do-everything",
            replaced: &["Shell", "Grep"],
        }]);

        // Then
        assert!(
            rendered.contains("Grep — handled by the `do-everything` subagent"),
            "non-shell tools keep the delegation clause: {rendered}"
        );
        assert!(
            rendered.contains("request_action"),
            "the shell replacement renders the actions surface: {rendered}"
        );
    }

    /// The still-available exec tools (e.g. Read) remain listed as the MUST-use set even when
    /// other tools are replaced — replacement narrows the set, it doesn't remove the preamble's
    /// guidance for what remains.
    #[test]
    fn preamble_with_replaced_tools_still_lists_the_remaining_tools() {
        // When
        let rendered = managed_codebase_preamble(&[SubagentReplacement {
            name: "explorer",
            replaced: &["Grep", "Glob"],
        }]);

        // Then
        assert!(
            rendered.contains("Read") && rendered.contains("Write"),
            "preamble must still list the remaining available tools: {rendered}"
        );
    }

    /// `create_with_subagent` prepends the subagent-aware preamble (not the plain one) to both
    /// CLAUDE.md and AGENTS.md.
    #[test]
    fn create_with_subagent_prepends_the_enforcement_paragraph_to_claude_and_agents_md() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# CLAUDE.md\n").unwrap();
        std::fs::write(source_dir.path().join("AGENTS.md"), "# AGENTS.md\n").unwrap();

        // When
        let ctx = SandboxContextDir::create_with_subagent(
            source_dir.path(),
            &[SubagentReplacement {
                name: "explorer",
                replaced: &["Grep", "Glob"],
            }],
            CLAUDE_GLOBS,
        )
        .expect("create_with_subagent must succeed");

        // Then
        for filename in ["CLAUDE.md", "AGENTS.md"] {
            let content = std::fs::read_to_string(ctx.path().join(filename))
                .unwrap_or_else(|_| panic!("{filename} must exist"));
            assert!(
                content.contains("explorer"),
                "{filename} must mention the replacing subagent: {content}"
            );
        }
    }

    /// `create(source_dir, globs)` is equivalent to `create_with_subagent(source_dir, &[], globs)`.
    #[test]
    fn create_without_subagent_omits_the_enforcement_paragraph() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# CLAUDE.md\n").unwrap();

        // When
        let ctx = SandboxContextDir::create(source_dir.path(), CLAUDE_GLOBS)
            .expect("create must succeed");
        let claude_md = std::fs::read_to_string(ctx.path().join("CLAUDE.md")).unwrap();

        // Then
        assert!(
            !claude_md.contains("not available") && !claude_md.contains("NOT available"),
            "plain create() must not include a tool-replacement paragraph: {claude_md}"
        );
    }

    #[test]
    fn sandbox_context_dir_copies_only_allow_listed_paths_not_the_full_repo() {
        // Given — repo layout with node_modules symlink that breaks naive fs::copy
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# project\n").unwrap();
        std::fs::write(source_dir.path().join("secret.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(source_dir.path().join("node_modules/.bin")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            "../../../secret.rs",
            source_dir.path().join("node_modules/.bin/secret"),
        )
        .unwrap();

        // When
        let ctx = SandboxContextDir::create(source_dir.path(), CLAUDE_GLOBS)
            .expect("create must succeed");

        // Then — guidance copied, codebase and node_modules omitted
        assert!(ctx.path().join("CLAUDE.md").exists());
        assert!(!ctx.path().join("secret.rs").exists());
        assert!(!ctx.path().join("node_modules").exists());
    }

    #[test]
    fn copy_tree_follows_symlink_to_directory_within_repo() {
        // Given
        let root = tempfile::tempdir().unwrap();
        let real_dir = root.path().join("real-skills");
        std::fs::create_dir_all(real_dir.join("nested")).unwrap();
        std::fs::write(real_dir.join("nested/skill.md"), "skill").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("real-skills", root.path().join("skills")).unwrap();
        #[cfg(not(unix))]
        return;

        let dest = tempfile::tempdir().unwrap();
        let mut visited = HashSet::new();

        // When
        copy_tree_within_root(
            &root.path().join("skills"),
            &dest.path().join("skills"),
            root.path(),
            &mut visited,
        )
        .expect("copy_tree_within_root");

        // Then
        let copied = std::fs::read_to_string(dest.path().join("skills/nested/skill.md")).unwrap();
        assert_eq!(copied, "skill");
    }

    #[test]
    fn copy_context_skips_symlink_outside_repo() {
        // Given
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join("CLAUDE.md"), "# ok\n").unwrap();
        std::fs::create_dir_all(source_dir.path().join(".claude")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("/etc/passwd", source_dir.path().join(".claude/leak")).unwrap();
        #[cfg(not(unix))]
        return;

        // When
        let ctx = SandboxContextDir::create(source_dir.path(), CLAUDE_GLOBS)
            .expect("create must succeed");

        // Then
        assert!(!ctx.path().join(".claude/leak").exists());
    }

    #[test]
    fn sandbox_spec_rejects_empty_command() {
        // Given
        let spec = SandboxSpec {
            project_root: "/tmp/project".into(),
            scratch_dir: "/tmp/project/.work".into(),
            egress_dir: "/tmp/project/out".into(),
            allow_read_paths: vec![],
            command: vec![],
            env: Default::default(),
            profile_path: "/tmp/project/profile.sb".into(),
            loopback_allow_ports: vec![],
            ipc_socket: None,
            cwd: None,
        };

        // When / Then
        assert!(spec.validate().is_err());
    }
}

# 2026-11-03 — `tddy-sandbox` still names the guidance files of two agents

**Category:** Unfinished (needs a design call)
**Source:** `#agent-tools` 10/12 (#799), `packages/tddy-sandbox/tests/no_agent_vocabulary.rs`
(`tddy_sandbox_and_tddy_sandbox_recipes_name_no_coding_agent`, red)

## What is left

`tddy-sandbox-recipes` is clean. `tddy-sandbox` still spells agent names in `src/`: the constants
`MANAGED_CODEBASE_PREAMBLE` and `PREAMBLE_FILES` (`["CLAUDE.md", "AGENTS.md"]`) in `context_dir.rs`,
and `.claude/**`, `CLAUDE.md`, `Claude Code` in the comments and in-source tests of `context_dir.rs`,
`context_manifest.rs`, `context_manifest/{links,walk}.rs`, `exec_reads.rs` and `builder.rs`.

## Why it is not a move

`PREAMBLE_FILES` is read *inside* the crate: `links.rs::is_preamble_path` (called by
`allow_listed_link` and `walk.rs`) and `context_dir.rs` (`create_with_subagent`,
`mark_context_stale`, `clear_context_stale`). Moving the constants moves a parameter, not a value:
the list has to reach the walk, so every function that carries `globs` today carries the guidance
files beside it — `walk_context_files`, `context_tree`, `collect_allow_listed`,
`resolve_preamble_link`, `context_reach`, `ContextManifest::of_worktree`, `allow_listed_link`,
`copy_context_from_repo`, `SandboxContextDir::create*`, `mark_context_stale`, `clear_context_stale`
— and about thirty call sites in `tddy-session-files`, `tddy-session-split`, `tddy-daemon-sandbox`,
`tddy-sandbox-app` and the acceptance suites change with them.

## Proposed shape

- `tddy-coding-agent-specs` gains `managed_codebase` holding `MANAGED_CODEBASE_PREAMBLE`,
  `PREAMBLE_FILES`, `SubagentReplacement` and `managed_codebase_preamble` (it already knows each
  agent's `instructions.files`); `context_preamble_unit.rs` moves with them.
- `tddy-sandbox` keeps `prepend_preamble` and the stale-marker functions, which are text operations,
  and takes `guidance_files: &[&str]` where it read `PREAMBLE_FILES`.
- The comments and in-source tests use neutral names (`RULES.md`, `.agent-config/**`).

`workspace_exec_tool_names` has already moved (to `tddy_coding_agent_specs::claude_tools`).

# 2026-11-21 — An orchestrator reads every tddy worktree, agent home and chat file on its host

**Type:** Feature · **Stack:** `#orchestra` 1/9 (PR #979) · **Packages:** `tddy-tool-engine`,
`tddy-daemon-rpc`, `tddy-session-activity`, `tddy-coding-agent-specs` ·
Feature: [project-concept.md § Read reach](../../ft/daemon/project-concept.md#read-reach)

## Problem

`contain_path` confined every daemon read to the caller's own worktree, so an orchestrator saw its
sibling sessions only through `ReadSessionWorktree` and could never read another project's code, an
agent's settings or a raw chat transcript.

## What changed

- **`tddy-tool-engine::reach`** (new): `ReadReach { home, roots, worktrees }` with `resolve` (relative
  under the home, absolute by component containment after symlinks, `..` refused, a refusal naming
  `RequestPathAccess`), `check_listed` (inside another worktree only what `git_listed_files` lists),
  `execute_in_reach` for `Read` / `Grep` / `Glob`. `read_resolved`, `grep_target` and `glob_where`
  were split out of the existing tools so both paths share one body.
- **`tddy-daemon-rpc::exec_tool::orchestrator_reach`** (new): for an orchestrator-role caller (and
  not a conversation), builds the reach per call — project worktrees from `git worktree list` of each
  project's main checkout on this host, agent `home_dirs` under the OS user's home and the agent's
  persistent home, the session's grants — and answers before the inbox-watch route. New internal
  dependency `tddy-actions` (for `HomeSection`).
- **`tddy-session-activity::path_grants`** (new): `PathGrantStore::{open, grants, add}`, `PathGrant`,
  `PathGrantMode`, persisted as `<session_dir>/path-grants.json` (atomic rename). Read here; written
  by `#orchestra` 6/9.
- **`tddy-coding-agent-specs`**: `coding_agent.home_dirs` slot, validated at load (no empty,
  absolute or `..` entry); declared in all five shipped specs.

## Decisions

- Enforcement is daemon-side in the exec tools, so it holds for every harness.
- Both orchestrator roles (`project-home`, `orchestrator`) share the wiring via `is_orchestrator_role`.
- Grants live with the session directory, so they survive a resume and die with the session.
- Found in validation and fixed here: the PRD promised `.git` and git-ignored paths of a worktree stay
  hidden; the first green read them. Other worktrees are now gated by git's listing (cached per call
  for `Glob`); agent homes and grants are read as-is.

## Code issues

- New record `packages/tddy-coding-agent-specs/docs/code-issues/oversized-file-spec.md`: `spec.rs`
  461 → 500 production lines; split deferred — every later `#orchestra` node edits that file.

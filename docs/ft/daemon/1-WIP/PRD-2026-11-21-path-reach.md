# An orchestrator reads every tddy worktree, agent home and chat file on its host - PRD

**Date**: 2026-11-21
**PRD Type**: Enhancement
**Stack**: `#orchestra` 1/9 — branch `feature/orchestra/path-reach`, base `feature/target-parity/target-worktree-stats`

## Affected Features

- **Primary Feature**: [Project concept — orchestrator sessions](../project-concept.md)

## Summary

An orchestrator-role session's daemon `Read`, `Grep` and `Glob` accept absolute paths and answer for any path inside its **reach**: every worktree of every tddy project on its host (its own project's included), and the home and chat directories each installed coding agent declares in its YAML. A path outside reach is refused with a message naming the reach and pointing at `RequestPathAccess` (n6). Reach also includes the session's **grants**, read from a per-session store that this node creates and n6 fills.

## Background

Today `contain_path` (`packages/tddy-tool-engine/src/lib.rs:107`) confines every read to the caller's own home worktree, so an orchestrator can see its sibling sessions only through `ReadSessionWorktree` (relative paths, own project) and can never read another project's code, an agent's settings, or a raw chat transcript.

## Proposed Changes

### What's Changing
- `Read` / `Grep` / `Glob` from an orchestrator role resolve an absolute path against its reach; a relative path keeps meaning its own home worktree.
- Reach = worktrees of all projects registered on the host (resolved per call, so a new session's worktree is readable at once) + every agent's `home_dirs` + the session's grants.
- Git-ignored and `.git` paths of a worktree stay hidden as in `ReadSessionWorktree`; agent homes are read as-is.
- A refusal names the nearest reach root and the `RequestPathAccess` tool.

### What's Staying the Same
- Non-orchestrator sessions: unchanged containment to their own worktree.
- Writes: still refused for orchestrator roles (n6 adds granted writes).
- The agent-side withdrawal of native tools (`NATIVE_FILESYSTEM_TOOLS`).

## Impact Analysis

### Technical Impact
- **tddy-tool-engine**: `contain_path` / `tool_read` take a reach instead of a single worktree root; absolute paths accepted for orchestrator roles
- **tddy-daemon-rpc**: `exec_tool/role_guard.rs` + `project_home/caller.rs` build the caller's reach and hand it to the read tools
- **tddy-session-activity**: `PathGrantStore` — per-session grants, persisted in the session directory (empty until n6 writes one)
- **tddy-coding-agent-specs**: new `coding_agent.home_dirs` slot naming each agent's home / chat directories (e.g. `~/.claude`, `~/.cursor`, Muse's home)
- **tddy-projects**: read-only: lists every project's worktrees on the host

### User Impact
An orchestrator can read the code of any session or project on its host, an agent's settings, and raw chat files — enough to review and plan without opening a session.

## Implementation Plan

One node: `feature/orchestra/path-reach` (#orchestra 1/9).

## Acceptance Criteria

- [ ] An orchestrator reads a file in another tddy project's worktree by absolute path.
- [ ] An orchestrator reads a file under a coding agent's declared home directory.
- [ ] A path outside reach is refused, and the refusal names `RequestPathAccess`.
- [ ] A path granted in the session's grant store is readable; the grant survives a resume.
- [ ] A non-orchestrator session's reads are unchanged.

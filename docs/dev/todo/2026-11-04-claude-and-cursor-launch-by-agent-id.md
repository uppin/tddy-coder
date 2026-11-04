# 2026-11-04 — Claude and Cursor still launch through their own paths, selected by agent id

**Category:** Gap
**Source:** `#agent-tools` 12/12 (#801), `TODO(agent-registry)` at `packages/tddy-agent-launch/src/harness_from_spec.rs`
(`CLAUDE_AGENT_ID`, `CURSOR_AGENT_ID`)

## What is there

`StartSession` and `ResumeSession` read the agent a `<id>-cli` session type names from the registry
and take the generic spec-driven launch for every agent but two: `claude` and `cursor` keep their
bespoke start, resume, tmux and recipe paths, picked by comparing the *spec id*. A session-type
string no longer gates anything, but those two agent ids still do. The branch policy of those two
paths (`BranchIntentPolicy::claude_cli()` / `cursor_cli()`) is also fixed in Rust, so removing
`branch_name_required` from `claude.yaml` does not change a claude start.

## What to decide

Fold the two paths into the generic launch (or declare the launcher in the spec) and read
`branch_name_required` from the spec in every path.

**Changeset ACs left open:** the claude/cursor halves of "every `session_type == "claude-cli"` gate
is `spec.has(…)`" for launch dispatch and branch intent (the scan pins comparisons of session-type
literals only).

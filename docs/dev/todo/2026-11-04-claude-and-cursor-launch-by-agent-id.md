# 2026-11-04 — Claude and Cursor still launch through their own paths, selected by agent id

**Category:** Gap
**Owner:** `#agent-tools` n14 `cli-spec-features` ([#820](https://github.com/uppin/tddy-coder/pull/820)) (`feature/agent-tools/cli-spec-features`: claude and cursor
features — managed workflow, hooks, slots, conversation id, ssh, tmux via `plan_host_pty` — on the generic CLI
spec path), then n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)) (`feature/agent-tools/cli-dispatch-collapse`: the
`CliLauncher`/`CLAUDE_AGENT_ID`/`CURSOR_AGENT_ID` dispatch and the hand-built paths deleted, branch intent from
the spec, child spawns through specs) — moved out of #801 by the developer on 2026-11-04, to be planned on top of
`feature/agent-tools/agent-registry`. Code markers: `TODO(cli-dispatch-collapse)`.
**Source:** `#agent-tools` 12/19 (#801), `TODO(cli-dispatch-collapse)` at `packages/tddy-agent-launch/src/harness_from_spec.rs`
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

**#801 ACs moved to n17 `cli-dispatch-collapse` ([#823](https://github.com/uppin/tddy-coder/pull/823)):** the claude/cursor halves of "every `session_type == "claude-cli"` gate
is `spec.has(…)`" for launch dispatch and branch intent (the scan pins comparisons of session-type
literals only).

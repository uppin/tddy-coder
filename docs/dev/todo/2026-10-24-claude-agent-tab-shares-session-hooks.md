# 2026-10-24 — A claude-cli agent tab reports its activity as the session's

**Category:** Deferred from "Agent terminal tabs run any CLI harness beside bash" (#718, `#warp-agent` 2/2)
**Source:** `packages/tddy-session-lifecycle/src/connection_service/svc_terminal_ports.rs` (`CliManagerTerminalRoster::start`, the `TerminalKind::Agent` arm)

## What happens

An agent terminal tab runs its harness in the session's worktree with no session environment and no
per-tab hooks. In a **claude-cli** session that worktree already carries the session's
`.claude/settings.local.json`, whose hooks report activity for the session. A Claude Code started in
a tab reads the same file, so its tool calls and stop events may be attributed to the main session's
activity status.

## Why deferred

#718's boundary excludes hooks and activity status for agent tabs (no `.claude/settings.local.json`
written or changed per tab). Other harnesses do not read that file, so only claude-cli tabs in a
claude-cli session are affected.

## What would close it

Give an agent tab its own activity identity: either run claude-cli tabs with a hooks settings file of
their own (`--settings`) whose events carry the tab's terminal id, or have the hook receiver drop
events whose Claude `session_id` is not the session's. Pin it with a
`agent_terminal_tabs_acceptance.rs` case: a claude stub in a tab fires a hook, and the session's
activity is unchanged.

# 2026-11-15 — A tool session cannot be named at its start

**Category:** Deferred scope
**Source:** [2026-11-15-session-display-name.md](../changesets/2026-11-15-session-display-name.md)

## What is wrong

`StartSessionRequest.display_name` names every session the daemon starts except one whose session
type is declared with the `tddy_coder` launch: that start is refused with `INVALID_ARGUMENT`
(`tddy_agent_launch::start_request_checks::refuse_a_named_tool_start`), and the create pane hides
its Name field for such a type. The session can be renamed once it runs — `RenameSession` from the
web or an orchestrator works on it like any other.

## Why it was not done with session names

A tool session's `.session.yaml` is written by its own `tddy-coder` process after the daemon spawns
it (`write_initial_tool_session_metadata`, called from `tddy-coder`'s `run.rs`), and that write
builds a fresh record. A name the daemon recorded after the spawn would race that write and be lost
whenever the child wrote second; one recorded before it would be overwritten. Every other start's
record is the daemon's own, so the daemon names those once, centrally, after the start returns.

## What would close it

Pass the name to the spawned `tddy-coder`, the way `--stack-parent` reaches it: a `display_name`
field on `tddy_spawn::spawner::SpawnOptions`, a `--display-name` flag on `tddy-coder`, and a
`display_name` on `InitialToolSessionMetadataOpts` that `write_initial_tool_session_metadata`
records. Then drop `refuse_a_named_tool_start`, and show the create pane's Name field for tool
session types too.

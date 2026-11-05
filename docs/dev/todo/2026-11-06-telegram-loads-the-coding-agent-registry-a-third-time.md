# 2026-11-06 — Telegram loads the coding-agent registry a third time

**Category:** Deferred from `#agent-tools` 18/19 (`feature/agent-tools/telegram-spec-launch`)
**Source:** `TODO(telegram-spec-launch)` at `packages/tddy-daemon/src/runtime/telegram_wiring.rs` (`telegram_coding_agents`)

## What is there

Telegram is built before the session host (`BuildContext::new` is infallible), so it cannot share
the host's registry. `telegram_coding_agents` calls `host_coding_agents` itself, and
`open_shared` and `build_session_host` each call it again: three loads of `<tddy_data_dir>/tools`
per daemon start. A registry that fails to load is logged and leaves Telegram with none (its CLI
starts are refused in the chat); the daemon's own start refuses the same file, so this is a report
and not a fallback.

## What would close it

Load the registry once, fallibly, before `Telegram::open` (`Registries::open` or a step ahead of
it), hand the same `Arc<HostCodingAgents>` to Telegram, `open_shared` and `build_session_host`, and
refuse the daemon's start there. Restructuring is the sweep node's (n20).

## Also left for the sweep

- `LaunchSessions::start_session_for_os_user` runs the generic start with no deadline of its own
  (`CancellationToken::new()`); Telegram bounds its call with a 600 s deadline
  (`cli_spawns.rs::START_DEADLINE`). Parent code, reported not fixed.
- `tddy-daemon/tests/session_agent_remote_acceptance.rs` (`seeds_a_peers_agent_onto_a_session_whose_codebase_is_here`,
  `fails_only_the_agents_of_a_daemon_that_goes_away`) start a `cursor-cli` without installing a
  cursor stub: "tool cursor is unavailable" on a machine with no `agent` binary.

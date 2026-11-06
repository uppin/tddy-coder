# 2026-11-06 — Telegram loads the coding-agent registry a third time

**Category:** Deferred from `#agent-tools` 18/19 (`feature/agent-tools/telegram-spec-launch`)
**Source:** `TODO(sweep)` at `packages/tddy-daemon/src/runtime/telegram_wiring.rs` (`telegram_coding_agents`)

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


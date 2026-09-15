# PRD — the Telegram control plane becomes its own crate

**Date:** 2026-09-15
**Stack:** `#carve` 7/9
**Packages:** `packages/tddy-session-lifecycle`, `packages/tddy-telegram-control` (new), `packages/tddy-daemon-kernel`, `packages/tddy-daemon`
**Product area:** [`docs/ft/daemon`](../../ft/daemon/)

## Problem

`tddy-session-lifecycle` is 38,083 lines — the remainder left by the `#unbundle` stack. **7,403 of
them (19%) are Telegram**, across six files, and they are the crate's only `teloxide` user:

| File | Total | Prod |
|---|---:|---:|
| `telegram_session_control.rs` | 4,476 | 3,980 |
| `telegram_notifier.rs` | 1,745 | 1,239 |
| `telegram_bot.rs` | 788 | 788 |
| `telegram_session_subscriber.rs` + `telegram_multi_select_shortcuts.rs` | 231 | 231 |
| `session_notification_subscribers.rs` | 163 | 163 |

They also hold the crate's two worst functions by CRAP score, both entirely untested:
`telegram_bot.rs:368 telegram_callback_handler` (complexity **88**) and
`telegram_bot.rs:211 telegram_message_handler` (complexity **52**).

### One field is the whole obstacle

Every outbound edge from the Telegram cluster is acyclic — it reaches `cli_session_manager`,
`session_deletion`, `session_list_enrichment`, `cursor_cli_spawn`, `session_reader`,
`project_storage`, `branch_owner`, `presenter_intent_client`, `active_elicitation`, `elicitation`,
`config` and `user_sessions_path`, and **none of those reaches back**.

The single back-edge is a field and its one call site:

```rust
// connection_service.rs:141
telegram: Option<Arc<TelegramDaemonHooks>>,

// connection_service/svc_resolve_tddy_tools_path.rs:439 — the only consumer
crate::telegram_session_subscriber::spawn_presenter_observer_task(
    self.telegram.clone(), publishing, session_id, grpc_port);
```

`TelegramDaemonHooks` is defined in `telegram_session_subscriber.rs`, which moves. So the field
**must** be inverted for the cluster to leave — this is the one design change in the node.

### The control file is one 2,634-line `impl`

`telegram_session_control.rs` is ~1,200 lines of DTOs and ~30 pure `parse_*` functions, then a
**single `impl TelegramSessionControlHarness` block spanning lines 1272–3906** with 57 methods.

## What this PR delivers

### FR1 — a `PresenterObserverSpawner` port

`ConnectionServiceImpl` holds a port — a trait object owned by `tddy-daemon-kernel`, where the
symbols every daemon subsystem shares already live — instead of a concrete `TelegramDaemonHooks`.
`tddy-daemon`'s `runtime.rs` injects the Telegram adapter. `connection_service` no longer names
anything Telegram.

### FR2 — `telegram_session_control.rs` splits

| Module | ~Lines | Content |
|---|---:|---|
| `callbacks.rs` | 700 | the ~30 pure `parse_*` / `chunk_telegram_text` functions and their tests |
| `commands.rs` | 150 | the 20 command/outcome DTOs |
| `spawn.rs` | 220 | `TelegramWorkflowSpawn` |
| `pickers.rs` | 600 | intent → project → branch → conflict → model → agent keyboards |
| `elicitation.rs` | 310 | select / other / answer-text / multi-select |
| `session_start.rs` | 700 | workflow / claude / cursor start and the two CLI spawners |
| `session_admin.rs` | 400 | chain, recipe, plan review, list / delete / enter |

The harness struct and its `impl RunnerHooks`-equivalent stay in `telegram_session_control.rs`.

### FR3 — `tddy-telegram-control`

All six files move to a new crate depending on `tddy-telegram` and `tddy-session-lifecycle`.
`teloxide` leaves `tddy-session-lifecycle`. `tddy-daemon`'s `lib.rs` facade re-points at the new
crate, so its 12 Telegram test suites (4,901 lines) are not edited.

**A new crate, not `tddy-telegram`**: that crate depends on `tddy-core`, `tddy-github`,
`tddy-daemon-kernel`, `tddy-rpc` and `tddy-service` — **not** on `tddy-session-lifecycle**, which
depends on *it*. Moving the control modules into it would close a cycle.

## Acceptance criteria

| # | Criterion |
|---|---|
| AC1 | `connection_service` names no Telegram symbol; the field is a `tddy-daemon-kernel` port |
| AC2 | `tddy-daemon`'s `runtime.rs` injects the Telegram adapter, and a session with Telegram disabled still starts |
| AC3 | `tddy-session-lifecycle/Cargo.toml` names neither `teloxide` nor any Telegram module |
| AC4 | `tddy-telegram-control` depends on `tddy-telegram` and `tddy-session-lifecycle`; neither depends back |
| AC5 | No module under `telegram_session_control/` exceeds 800 production lines |
| AC6 | Every `tddy_daemon::telegram_*` path resolves — the 12 daemon test suites are not edited |
| AC7 | `./test -p tddy-session-lifecycle -p tddy-telegram-control` passes at baseline test counts |

## Out of scope

- **Testing the two high-CRAP handlers.** `telegram_callback_handler` (88) and
  `telegram_message_handler` (52) move **unchanged and still untested**. Decomposing them is a
  behaviour-risk change that does not belong in a move, and the backlog entry recording them stays
  open with a note that they relocated.
- `connection_service/`'s other 64 files.
- Any change to Telegram command syntax, callback payloads or message formatting.

## Why this needs 1/9 and 3/9

- **1/9** — the moves leave `pub use` facades in `tddy-session-lifecycle` and in `tddy-daemon`.
- **3/9** — `telegram_notifier ↔ telegram_session_control` and
  `telegram_notifier ↔ telegram_multi_select_shortcuts` are **mutual**. This is the genuine entangled
  cluster the leaf-first trick cannot solve, and the one node in the stack that truly needs
  multi-module moves.

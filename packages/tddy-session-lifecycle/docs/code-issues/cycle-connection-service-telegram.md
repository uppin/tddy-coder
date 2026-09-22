# cycle: connection_service ↔ the Telegram cluster — one field

**Location:** `packages/tddy-session-lifecycle/src/connection_service.rs:141`
**Category:** cycle
**Detected:** 2026-09-15 by structural audit
**Metrics:** **1 field** · **1 call site** · blocks **7,403 lines** (19% of the crate) from leaving
**Restructure:** required — replace the field with an injected port
**Status:** Closed by #494 — delete at wrap, once this final measurement is in the change-history entry
**Claimed by:** #494 — `#carve` 8/11 `telegram` · draft · `feature/carve/telegram`
**Lands after:** #488, #489, #490, #498, #491, #492, #493

## Measurement history

| Run | Fields | Call sites | Lines blocked | Note |
|---|---|---|---|---|
| 2026-09-15 | 1 | 1 | 7,403 | first detection |
| 2026-09-22 | 0 | 0 | 0 | closed by #494: `DaemonSessionHost` holds `presenter_event_sink: Option<SharedPresenterEventSink>`; the cluster is in `tddy-telegram-control` |

## What the tool found

Every outbound edge from the Telegram cluster is acyclic — it reaches `cli_session_manager`,
`session_deletion`, `session_list_enrichment`, `cursor_cli_spawn`, `session_reader`,
`project_storage`, `branch_owner` and `presenter_intent_client`, and **none of those reaches back**.

The single back-edge is a field and its one consumer:

```rust
// connection_service.rs:141
telegram: Option<Arc<TelegramDaemonHooks>>,

// connection_service/svc_resolve_tddy_tools_path.rs:439 — the ONLY consumer
crate::telegram_session_subscriber::spawn_presenter_observer_task(
    self.telegram.clone(), publishing, session_id, grpc_port);
```

`TelegramDaemonHooks` is **defined in `telegram_session_subscriber.rs`** — one of the six modules the
cluster consists of.

## Why it matters here

One field holds 19% of the crate in place. The cluster is also the crate's only `teloxide` user, so
that dependency is in every build of it for the same reason.

## What would close it

A `PresenterObserverSpawner` port in `tddy-daemon-kernel` — where the symbols every daemon subsystem
shares already live, because `pub(crate)` does not cross a crate boundary. `tddy-daemon`'s
`runtime.rs` injects the adapter; `connection_service` holds `Arc<dyn _>` and names nothing Telegram.

`Option<…>` being `None` is a first-class state today (a daemon with no `telegram:` block has no
hooks), so ship a `NoPresenterObserver` implementation and let the service hold **one shape** rather
than branching on absence.

## How it was closed (#494)

Not with the `PresenterObserverSpawner` proposed above. The observer has **two** independent sinks
— the Telegram chat surface and the session-notification bus that lights the drawer indicator —
and runs when either exists, so a Telegram-owned spawner would have left the indicator dark on
every Telegram-less daemon. Only the Telegram half was inverted: the loop stayed in
`presenter_observer_task.rs`, and `tddy-daemon-kernel::presenter_observer::PresenterEventSink` is
what `TelegramDaemonHooks` implements. The service holds an honest `Option`, because the loop's
"neither sink, don't spawn" rule needs to know there is no sink.

## If you are about to change this code

`connection_service.rs` is the crate's central service and #494 touches **one field and one call
site** in it. Almost any concurrent change to `connection_service` is unaffected.

Coordinate only if you are **adding another concrete notifier dependency** to
`ConnectionServiceImpl` — that is the same mistake this issue records, and it should be a port from
the start.

## Verified by hand

2026-09-15: grepped every use of the field (three hits: the declaration, a constructor parameter, and
the one call site) and confirmed `TelegramDaemonHooks` is defined in a moving module. Also confirmed
the apparent `tddy_daemon::` references in this file are **log-target string literals**, not paths —
`tddy-daemon` is not a dependency of this crate, so there is no reverse edge.

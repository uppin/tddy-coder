# Changeset: carve-telegram

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor + Architecture Change
**Stack**: `#carve` 8/10
**PR**: [#494](https://github.com/uppin/tddy-coder/pull/494)

PRD: [`2026-09-15-carve-telegram-prd.md`](./2026-09-15-carve-telegram-prd.md)

## Initial Discovery

[`2026-09-15-carve-telegram-initial-discovery.md`](./2026-09-15-carve-telegram-initial-discovery.md)

## Affected Packages

- **`tddy-telegram-control`** (new): the Telegram modules (`telegram_bot`, `telegram_notifier`,
  `telegram_multi_select_shortcuts`, `telegram_session_subscriber`, the split
  `telegram_session_control/`) plus `telegram_notification_subscriber` — 7,403 lines less the
  observer loop, which stayed — and 15 test suites.
- **`tddy-session-lifecycle`**: loses the cluster and the `teloxide` line from its manifest; gains
  `presenter_observer_task.rs`. (It has no `README.md`; the link this line used to carry was dead.)
- **`tddy-daemon-kernel`**: `presenter_observer` becomes the `PresenterEventSink` port.
- **`tddy-daemon`**: `runtime.rs` and `server.rs` re-point to `tddy_telegram_control`, and
  `runtime.rs` injects the hooks as the sink. `lib.rs` needed nothing — it re-exported no Telegram
  module.

## Responsibility

- Replace `ConnectionServiceImpl`'s `telegram: Option<Arc<TelegramDaemonHooks>>` with a
  `tddy-daemon-kernel` port — `presenter_event_sink: Option<SharedPresenterEventSink>` — injected by
  `tddy-daemon`'s `runtime.rs`.
- Keep the presenter observer loop in `tddy-session-lifecycle` (`presenter_observer_task.rs`); only
  its Telegram sink is inverted.
- Split `telegram_session_control.rs` (3,980 prod, one 2,634-line `impl`) into seven modules.
- Move the Telegram modules — and `TelegramNotificationSubscriber`, the straggler in
  `session_notification_subscribers.rs` that named `TelegramDaemonHooks` — into a new
  `tddy-telegram-control`.
- Move the 12 Telegram suites, and the three further suites that are Telegram at heart
  (`session_notifications_acceptance`, `session_chaining_phase2_{acceptance,unit}`), from
  `tddy-session-lifecycle/tests/` into `tddy-telegram-control`, with the code they exercise.

## Boundaries

- Does **not** test or decompose `telegram_callback_handler` (CRAP 88) or
  `telegram_message_handler` (CRAP 52). They move **unchanged and still untested**; the backlog entry
  recording them stays open, annotated with their new home.
- Does **not** touch `connection_service/`'s other 64 files.
- Does **not** change Telegram command syntax, callback payloads or message formatting.
- Does **not** move the control modules into the existing `tddy-telegram` — that would close a cycle.
- Does **not** rewrite what the Telegram test suites assert. They move; their assertions do not.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | facade-aware cycle refusal; nested anchors | the moves leave `pub use` facades in two crates | touch `tddy-code-restructuring` |
| `3/9` restructure-clusters | **multi-module cluster moves** | `telegram_notifier ↔ telegram_session_control` and `telegram_notifier ↔ telegram_multi_select_shortcuts` are **mutual**. This is the one node in the stack that genuinely needs it | rely on leaf-first ordering |
| `4/9`, `5/9`, `6/9` | `tddy-core` changes | **not consumed** | touch `tddy-core` |

## Draft PR contract

Published first:

**Published** (commit 2): `tddy-daemon-kernel/src/presenter_observer.rs` —
`PresenterObserverSpawner`, `SharedPresenterObserver` and `NoPresenterObserver`. It lives in the
kernel beside the other symbols every daemon subsystem shares, for the same reason that crate
exists: `pub(crate)` does not cross a crate boundary.

**Reshaped during `/green`** (developer decision) — the published spawner was the wrong inversion.
`spawn_presenter_observer_task` takes **two independent sinks**: the Telegram hooks and
`SessionNotificationPublishing`, the bus that lights a workflow session's drawer indicator. It runs
when *either* exists. A Telegram-owned spawner would have taken the bus publish with it, and every
Telegram-less daemon — most of them — would have lost the indicator. So only the Telegram half is
inverted:

```rust
#[async_trait]
pub trait PresenterEventSink: Send + Sync {
    async fn on_presenter_event(&self, session_id: &str, event: &ServerMessage)
        -> anyhow::Result<()>;
}
pub struct NoPresenterEventSink;
pub type SharedPresenterEventSink = Arc<dyn PresenterEventSink>;
```

- The loop (connect with retry, stream, bus publish, log targets, the "neither sink → do not spawn"
  rule) stays in `tddy-session-lifecycle/src/presenter_observer_task.rs`, unchanged but for taking
  `Option<SharedPresenterEventSink>` where it took the hooks.
- `TelegramDaemonHooks` implements the sink with the loop's former Telegram body — lock the watcher,
  `on_server_message`. An error ends the loop exactly as the `?` did.
- The service holds an honest `Option`, not a no-op: the spawn rule has to know there is no sink, and
  injecting `NoPresenterEventSink` would start an observer on a daemon with nothing to deliver to.
  The no-op exists for callers that must hand over *a* sink.

**Consequence the plan did not anticipate:** `DaemonSessionHost::new` used to build a Telegram-only
notification bus from the hooks it was given. A service holding a port cannot name
`TelegramNotificationSubscriber`, so `new` now installs no bus. `runtime.rs` always replaced that
default with its own bus, so the daemon is unchanged; the one suite that relied on it
(`telegram_claude_cli_activity_alert_acceptance`) installs the bus the way `runtime.rs` does, with
its assertions untouched.

The seven control modules are a **split**, not new API, so they are pinned by
`tests/telegram_extraction_shape.rs` rather than declared.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 4 of 5
**Greenable independently:** **no** — the cluster is mutually referencing, so it cannot move until
`#carve` 3/9's multi-module support exists as behaviour. The `git mv` fallback exists but is what
this stack was built to avoid
**Concurrent with:** `#carve` 6/9 `session-store`, 8/9 `presenter-split`
**Blocks:** nothing

Real dependency edges, as refined by `#carve` 6/9's discovery:

    n1 → n3, n4, n5, n6, n9      n3 → n7, n9      n4 → n6, n8, n9      n5 → n9

## Prerequisites

| Entry | Verdict | What this node does with it |
|---|---|---|
| [2026-09-09-tddy-daemon-untested-complexity-hotspots.md](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | Records `telegram_callback_handler` (CRAP 7,832, complexity 88) and `telegram_message_handler` (2,756, complexity 52) as the crate's two worst, both untested, and names `telegram_bot.rs` "the first target for either tests or decomposition". **This node does neither** — it relocates them unchanged. **Annotate the entry with their new crate**; do not claim it. |
| [2026-08-29-session-notifications-three-follow-ups-left-open.md](../todo/2026-08-29-session-notifications-three-follow-ups-left-open.md) | ⚠ **DURING** | Concerns the notification bus this node's subscriber sits on. The subscriber moves; its behaviour must not. Not claimed. |
| [2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md](../todo/2026-09-12-session-agent-clone-is-two-daemons-in-one-module.md) | — Unrelated | Different module; no overlap. |

## State A → State B

### State A

- 7,403 Telegram lines in `tddy-session-lifecycle` (19% of the crate), the only `teloxide` user.
- `connection_service.rs:141` holds `telegram: Option<Arc<TelegramDaemonHooks>>`; its **one**
  consumer is `svc_resolve_tddy_tools_path.rs:439`, calling `spawn_presenter_observer_task`.
- `TelegramDaemonHooks` is defined in `telegram_session_subscriber.rs` — a module that moves.
- `telegram_session_control.rs` — 3,980 prod lines; one `impl` block, lines 1272–3906, 57 methods.
- Mutual references: `telegram_notifier ↔ telegram_session_control`,
  `telegram_notifier ↔ telegram_multi_select_shortcuts`.
- `tddy-telegram` exists but depends on `tddy-core`, `tddy-github`, `tddy-daemon-kernel`, `tddy-rpc`,
  `tddy-service` — and `tddy-session-lifecycle` depends on **it**.

### State B

- `connection_service` names no Telegram symbol; `tddy-daemon` injects the adapter.
- `telegram_session_control.rs` is seven modules, none over 800 prod lines.
- `tddy-telegram-control` holds the cluster; `tddy-session-lifecycle` has no `teloxide`.

## Implementation phases

The stack's fullest **mechanical → manual → mechanical** node.

| Phase | Kind | Work |
|---|---|---|
| **A** | mechanical | `extract_module --to_file` × 7 on `telegram_session_control.rs`. **`callbacks.rs` first** — the ~30 `parse_*` functions are free functions with no `self`, the cheapest seam, and lifting them shrinks the file 700 lines before anything harder is attempted |
| **B** | manual | The `PresenterObserverSpawner` port in `tddy-daemon-kernel`; `connection_service`'s field; `runtime.rs`'s injection. **This is the design change** and the only hand-written behaviour in the node |
| **C** | manual | `tddy-telegram-control`'s skeleton — `Cargo.toml`, `lib.rs`, workspace `members` |
| **D** | mechanical | `move_module_to_crate` as a **cluster** — all six modules as one unit, `reexport: "glob"`. Requires `#carve` 3/9 |
| **E** | manual | `tddy-daemon`'s `lib.rs` facade re-pointed; `teloxide` dropped from `tddy-session-lifecycle`; `README.md` × 2; annotate the CRAP backlog entry |

**As delivered:** `move_module_to_crate` was not attempted — it refused every cross-crate move
earlier in this stack — and neither was `extract_module`; both A and D were done by hand, A as a
scripted line-range split and D with `git mv`. Identity evidence is in the commit messages: A's
sorted line multiset differs from the original only in visibility, `mod`/`use`/`impl` wrapper and
`//!` lines; D's whole-file diffs are path rewrites and the rustfmt rewraps they caused.

| Module (`telegram_session_control/`) | Prod lines |
|---|---:|
| `callbacks.rs` | 736 |
| `session_start.rs` | 711 |
| `pickers.rs` | 705 |
| `mod.rs` | 539 |
| `chaining_and_listing.rs` | 518 |
| `elicitation.rs` | 456 |
| `workflow_spawn.rs` | 375 |

`teloxide` is gone from `tddy-session-lifecycle`'s manifest but still in its build graph, through
`tddy-telegram` (and `tddy-session-activity` → `tddy-telegram`): `session_list_enrichment` reads
`tddy_telegram::elicitation`, and the crate keeps its four `tddy_telegram` re-exports. AC3 is about
the manifest, which is what the test checks.

Phase A runs **before** Phase B deliberately: the port change touches
`svc_resolve_tddy_tools_path.rs` and `connection_service.rs`, and doing it after the 3,980-line file
has been carved keeps the two diffs separable for review.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [x] Publish the draft-PR contract (port signature + module shape + failing tests)
- [x] Failing acceptance tests — **USER REVIEW** (approved 2026-09-15, gates delegated)
  - `tddy-daemon-kernel/tests/telegram_extraction_shape.rs` — 5 failing (`connection_service` still
    names `TelegramDaemonHooks`; the spawn path still calls the subscriber directly;
    `tddy-session-lifecycle` still declares `teloxide` and holds six `telegram_*` modules;
    `tddy-telegram-control` does not exist; the control plane is unsplit). **1 passing**:
    `the_port_is_a_trait_object_the_service_can_hold` — the port is real now, which is what the rest
    of the node is built on.
- [x] Failing unit/integration tests — the same suite; AC2's Telegram-disabled start is exercised by `NoPresenterObserver`, which is the configuration it describes
- [x] Implement production code making tests pass (`/green`) — `telegram_extraction_shape` 6/6;
  the port reshaped to `PresenterEventSink` (see Draft PR contract)
- [x] Annotate the CRAP backlog entry with the handlers' new crate
- [x] Code-issue records: the cycle and oversized-file records closed with a final measurement
  (delete at wrap); the seven open Telegram records moved to `tddy-telegram-control/docs/code-issues/`
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-session-lifecycle -p tddy-telegram-control -p tddy-daemon-kernel
cargo clippy -p tddy-session-lifecycle -p tddy-telegram-control -p tddy-daemon-kernel --all-targets -- -D warnings
./test -p tddy-telegram-control   # proves AC6 — the 15 suites pass from their new home
cargo fmt --all --check
```

The 12 Telegram acceptance suites (4,901 lines) are the real regression gate here. **`#carve` 4/10
changed where they live and therefore what this criterion says**: they were `tddy-daemon`'s, reaching
the cluster through a facade, and the original AC6 asked that they pass *unedited through it*. After
4/10 they are `tddy-session-lifecycle`'s, so this node moves them **with the code** into
`tddy-telegram-control` — code and tests as one vertical slice, which is what the boundary contract
wanted in the first place. Their assertions still must not change.

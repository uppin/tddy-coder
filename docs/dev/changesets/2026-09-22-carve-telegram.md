# 2026-09-22 — The Telegram control plane leaves `tddy-session-lifecycle` for `tddy-telegram-control`, behind a presenter-event port

**Type:** Refactor

`#carve` 8/11, PR [#494](https://github.com/uppin/tddy-coder/pull/494). Builds on `#carve` 3/10
([#490](https://github.com/uppin/tddy-coder/pull/490)), `#carve` 4/10
([#498](https://github.com/uppin/tddy-coder/pull/498)), which gave `tddy-session-lifecycle` its
test suites, and `#carve` 7/11 ([#493](https://github.com/uppin/tddy-coder/pull/493)).

**7,403 lines of `tddy-session-lifecycle` (19%) were Telegram**, in six files, and they were the
crate's only `teloxide` user. Every outbound edge from the cluster was acyclic. The single back-edge
was one field, `telegram: Option<Arc<TelegramDaemonHooks>>` on the connection service, and its one
call site. `TelegramDaemonHooks` was defined in a module that had to move, so the field had to be
inverted before anything else could leave.

| Crate | Change |
|---|---|
| [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) | gains the port `presenter_observer::{PresenterEventSink, SharedPresenterEventSink}` |
| `tddy-session-lifecycle` | `DaemonSessionHost` holds `presenter_event_sink: Option<SharedPresenterEventSink>`. The observer loop stays, as `presenter_observer_task.rs`. The Telegram modules, `teloxide` and 15 test suites leave. Source 38,960 → 31,664 lines; tests 98 suites + `tests/common/mod.rs` → 83 suites |
| [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md) (new) | `telegram_bot`, `telegram_notifier`, `telegram_multi_select_shortcuts`, `telegram_session_subscriber` (now only `TelegramDaemonHooks` and its sink impl), `telegram_notification_subscriber`, and `telegram_session_control/` split into seven files. 7,408 source lines, 15 suites |
| `tddy-daemon` | `runtime.rs` and `server.rs` name `tddy_telegram_control`. `runtime.rs` injects the hooks as the sink and builds the bus with Telegram's subscriber. `lib.rs` needed nothing, because it re-exported no Telegram module |
| `tddy-testing-commons` | gains `wait::{a_capture_showing, PTY_STUB_OUTPUT}` |
| `tddy-telegram` | crate docs corrected; its `TODO(unbundle-node-2, M4)` removed as resolved |

Nothing changes behaviour. Command syntax, callback payloads, message text and log targets are
unchanged: targets still read `tddy_daemon::telegram*`, because an operator's `log:` filter is an
observable interface. The moved suites' assertions are unchanged; their diffs are crate paths.
Neither `move_module_to_crate` nor `extract_module` was used. The split was a scripted line-range
slice, whose sorted line multiset differs from the original only in visibility, the
`mod`/`use`/`impl` wrapper lines and `//!` lines. The crate move was `git mv`, whose diffs are path
rewrites and the rustfmt rewraps they caused.

## The split of `telegram_session_control.rs`

It was 3,980 production lines, with a single 2,634-line `impl` of 57 methods. Production lines
below are counted to the first `#[cfg(test)]`.

| Module | Prod lines |
|---|---:|
| `callbacks.rs` | 736 |
| `session_start.rs` | 711 |
| `pickers.rs` | 705 |
| `mod.rs` | 539 |
| `chaining_and_listing.rs` | 518 |
| `elicitation.rs` | 456 |
| `workflow_spawn.rs` | 375 |

AC5's ceiling was 800, and every module is under it. Five are over the repo's 500 budget, and each
now has its own record (see below).

## Developer decisions

1. **The port is an event sink, not an observer spawner.** The draft contract published a
   spawner port. At `/green` it turned out that `spawn_presenter_observer_task` feeds **two
   independent sinks**: Telegram, and `SessionNotificationPublishing`, the bus that lights a
   workflow session's drawer indicator. It runs when either exists. A Telegram-owned spawner would
   have taken the bus publish with it, and the indicator would have gone dark on every
   Telegram-less daemon. Only the Telegram half was inverted:
   `PresenterEventSink::on_presenter_event(&self, session_id, &ServerMessage) -> anyhow::Result<()>`.
   The service holds an honest `Option`, because the "neither sink, do not spawn" rule has to know.
   The no-op implementation `/green` added had no production caller and was removed at `/pr-wrap`.
2. **The stragglers moved with the cluster.** `TelegramNotificationSubscriber` sat in
   `session_notification_subscribers.rs` and named `TelegramDaemonHooks`, so it became
   `tddy_telegram_control::telegram_notification_subscriber`. Three suites that are Telegram at
   heart moved with the 12 Telegram suites: `session_notifications_acceptance`,
   `session_chaining_phase2_acceptance` and `session_chaining_phase2_unit`.
3. **`DaemonSessionHost::new` installs no default bus.** It used to build a Telegram-only
   notification bus from the hooks it was given. A host holding only the port cannot name
   `TelegramNotificationSubscriber`. `runtime.rs` always replaced that default, so the daemon is
   unchanged. The one suite that relied on it, `telegram_claude_cli_activity_alert_acceptance`,
   installs the bus the way `runtime.rs` does, with its assertions untouched.
4. **Keep the seven modules, and defer the five over budget.** The developer consented at
   `/pr-wrap` ("move-only node; developer consent 2026-09-22 in /pr-wrap"). Re-splitting would
   have made the diff no longer a verifiable slice of the old file. Three of the five cannot reach
   500 by moving items, because the size is in long functions on untested code. Recorded in
   `docs/dev/todo/2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node.md`.
5. **The PTY wait is hoisted into `tddy-testing-commons`.** Moving the Telegram start suites
   would have left a byte-identical `tests/common/mod.rs` in both `tddy-session-lifecycle` and
   `tddy-telegram-control`. Instead it became
   `tddy_testing_commons::wait::{a_capture_showing, PTY_STUB_OUTPUT}`, taking the PTY's
   `&Mutex<TerminalCapture>`, and both copies were deleted. Call sites changed `&handle` to
   `&handle.capture`, and their assertions are unchanged.

## Premises that were wrong

- **The spawner port** (decision 1). The plan and the cycle record both prescribed a
  `PresenterObserverSpawner` with a `NoPresenterObserver` default. Both would have broken the
  indicator on Telegram-less daemons.
- **AC6's "12 daemon suites pass unedited through `tddy-daemon`'s facade."** `#carve` 4/10 had
  already moved them into `tddy-session-lifecycle`, and `tddy-daemon`'s `lib.rs` re-exported no
  Telegram module at all. The suites moved **with the code** into `tddy-telegram-control`. Their
  assertions are unchanged, and their `use` paths were edited.
- **"`teloxide` leaves `tddy-session-lifecycle`."** It leaves the manifest, which is what AC3
  checks. It stays in the crate's build graph through `tddy-telegram`, because
  `session_list_enrichment` reads `tddy_telegram::elicitation` and the crate keeps four
  `tddy_telegram` re-exports.
- **FR2's module plan** (`commands`, `spawn`, `session_admin`, …, none over ~700). The delivered
  cut follows the file's own seams: seven different modules, the largest 736.
- **The mechanical phases.** A and D were planned as `extract_module --to_file` and a
  `move_module_to_crate` cluster move, which is the one place in the stack `#carve` 3/10's cluster
  support was meant to be consumed. `move_module_to_crate` had refused every cross-crate move
  earlier in the stack, so it was not attempted, and both phases were done by hand.

## Code issues closed

Claimed by this PR, re-measured at wrap, and their files deleted.

| Record | At detection (2026-09-15) | At wrap (2026-09-22) |
|---|---|---|
| `tddy-session-lifecycle`: `cycle-connection-service-telegram` | **1 field** (`connection_service.rs:141`, `telegram: Option<Arc<TelegramDaemonHooks>>`) · **1 call site** (`svc_resolve_tddy_tools_path.rs:439`) · blocks **7,403 lines** (19% of the crate) | **0 · 0 · 0.** `grep -rnE 'TelegramDaemonHooks\|telegram_session_subscriber\|TelegramSessionWatcher\|TelegramNotificationSubscriber' packages/tddy-session-lifecycle/src` → no match. The field is `presenter_event_sink: Option<SharedPresenterEventSink>` (`connection_service.rs:142`), and its one consumer is `svc_resolve_tddy_tools_path.rs:451`, through the port. `src/telegram*` → no files. `teloxide` and `tddy-telegram-control` are absent from the manifest. `telegram_extraction_shape` 9/9 |
| `tddy-session-lifecycle`: `oversized-file-telegram-session-control` | **3,980 production lines** (4,476 total) · one `impl` spanning 1272–3906 · 57 methods · budget 500 | The file is gone from the crate. `packages/tddy-telegram-control/src/telegram_session_control/` is seven files, the largest **736** production lines (table above), and the harness methods are spread over five `impl` blocks. The five over 500 are new records (below), so the oversize is narrowed and moved, not closed outright |

## Code issues moved, opened, updated

- **Moved** to `packages/tddy-telegram-control/docs/code-issues/`, each with a `**Moved:**` line
  and the metrics unchanged, because the code moved unchanged:
  `complexity-telegram-bot-telegram-callback-handler`, `crap-telegram-bot-handlers` (CRAP 7,832
  and 2,756, both still never executed), `complexity-telegram-notifier-on-metadata-tick`,
  `…-on-server-message`, `…-send-mode-changed-action-lines`,
  `complexity-telegram-session-control-handle-elicitation-multi-select-shortcut` (now in
  `elicitation.rs:273`) and `…-handle-telegram-branch-callback` (now in `pickers.rs:220`).
- **Opened:** `oversized-file-telegram-session-control-{callbacks,session-start,pickers,mod,chaining-and-listing}`
  at 736 / 711 / 705 / 539 / 518, and `oversized-file-telegram-notifier` at 1,246 production lines
  (1,239 before the move, +7 of rustfmt rewraps).
- **Updated:** `tddy-daemon`'s `oversized-file-runtime` (1,519 → 1,521) and
  `complexity-runtime-build` (831 → 833 lines, the sink cast; 806 at detection, so marked
  regressed). `tddy-session-lifecycle`'s `oversized-file-connection-service` gains an
  unchanged-in-substance row (+1 line net). The remaining `tddy-session-lifecycle` records name
  files this PR did not touch.

## Backlog

No `docs/dev/todo/` entry is resolved here.

- `2026-09-09-tddy-daemon-untested-complexity-hotspots` stays open and unclaimed. It is annotated
  with the handlers' new home, `packages/tddy-telegram-control/src/telegram_bot.rs:368` and `:211`.
  They moved unchanged and still untested.
- `2026-09-22-telegram-control-plane-left-over-budget-by-a-move-only-node` is new (decision 4).
- `2026-08-29-session-notifications-three-follow-ups-left-open` was read. The subscriber moved and
  its behaviour did not change.

## Verification

Scoped to the packages touched. `./test -p tddy-session-lifecycle -p tddy-telegram-control -p
tddy-daemon-kernel`, `cargo clippy` on the same three with `--all-targets -D warnings`, and
`cargo fmt --all --check`. `tddy-daemon-kernel/tests/telegram_extraction_shape.rs` passes **9/9**
at wrap:

- the shared port delivers to the sink behind it (a recording sink)
- `connection_service` names nothing Telegram, holds the port, and its one consumer goes through it
- `tddy-session-lifecycle` declares no `teloxide` in any dependency table and holds no `telegram_*`
  module, found by a recursive walk
- `tddy-telegram-control` depends on `tddy-telegram` and `tddy-session-lifecycle`, and neither
  depends back
- no `telegram_session_control` module exceeds 800 production lines

Manifests are parsed with `toml` rather than matched as substrings.

# 2026-09-09 — Model registry, telegram and screen sharing in their own crates

**Type:** Refactor

`#unbundle` node **2 of 8** ([#471](https://github.com/uppin/tddy-coder/pull/471), based on node 1's
`feature/unbundle/host-worktree-services`). Three subsystems leave `tddy-daemon` for crates of their
own, and one dead one is deleted. All three keep the proto services they already had, so **no RPC
coordinate moved and no client migrated**.

## What landed

| | |
|---|---|
| `tddy-model-registry` *(new)* | 13 modules serving `models.ModelRegistryService` (12 rpcs) and the daemon's model-addressed `tddy.acp.v1.AcpService`; 5 test suites moved with it |
| `tddy-screen-sharing` *(new)* | 2 modules serving `screen_sharing.ScreenSharingService` (10 rpcs) + the Argon2id/ChaCha20-Poly1305 credential vault; 2 test suites moved with it |
| `tddy-telegram` *(new)* | the transport seam plus 4 leaf modules — **partial by measurement, see below** |
| `tddy-daemon` | **152 → 131** source modules, **160 → 151** test files; `runtime.rs` registers all three through `build_*_entry` constructors instead of naming daemon-internal types |
| deleted | `vnc_service.rs`, `vnc_vault.rs` and their two acceptance suites — **1,008 lines reachable by nothing** |
| `tddy-service` | **no proto change.** `vnc.proto` is left in place with no server |

Docs: [`model-registry.md`](../../../packages/tddy-model-registry/docs/model-registry.md),
[`telegram-notifier.md`](../../../packages/tddy-telegram/docs/telegram-notifier.md),
[`telegram-github-link.md`](../../../packages/tddy-telegram/docs/telegram-github-link.md).

## The VNC service was deleted because nothing could reach it

`runtime.rs` registered `screen_sharing.ScreenSharingService` and **never** `vnc.VncService`. The
only references anywhere under `packages/` were the daemon's own `lib.rs` declarations, the two
source files and their two acceptance suites — code that was compiled, tested and unreachable.

The deletion rode with this node rather than getting one of its own because this is the only node
whose reviewer is already reading the live screen-sharing service beside it, which is the only
context in which "the VNC service is dead, and here is the live one" is checkable.

Two tests hold the removal from regressing, from opposite directions:
`tddy-screen-sharing/tests/dead_vnc_service_removed.rs` asserts the four **files** are gone, and
`tddy-daemon/tests/service_registration_acceptance.rs` builds the runtime and asserts
`vnc.VncService` is **not** among `DaemonRuntime::service_names()` while the three moved services
are. The source-level assertion is not redundant with the registry one: `vnc.VncService` was already
absent from the registry before the deletion, which is precisely what was wrong with it.

**The schema stays, and that turned up a live client with no server.** `vnc.proto` still compiles,
and `packages/tddy-web`'s session-inspector **"vnc" tab still dials `VncService`** — user-reachable
UI calling a service no daemon has ever registered. Retiring the proto means removing that tab, which
is a product-visible change and belongs in its own PR.
Recorded in [`docs/dev/todo/`](../todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md).

## Telegram is a partial delivery, and the reason is a real cycle

The plan was to move all nine telegram modules. **Four moved, plus the transport half of a fifth;
five stayed.**

Moved into `tddy-telegram`: `sender.rs` — the transport half of `telegram_notifier.rs` (the
`TelegramSender` port, `TeloxideSender`, `InMemoryTelegramSender` and the lifecycle announcement) —
plus `active_elicitation`, `elicitation`, `telegram_tracked_session` and `telegram_github_link`.
`telegram_tracked_session` is byte-identical; across the other three, **7 lines differ and every one
is an import path or a doc reference**.

Stayed in `tddy-daemon`: `telegram_session_control` (4,472 lines), `telegram_bot`,
`telegram_session_subscriber`, `telegram_multi_select_shortcuts`, and `telegram_notifier`'s
`TelegramSessionWatcher`.

The blocker is not size. `telegram_session_control` is an **orchestrator of the daemon's session
lifecycle** — it holds `CliSessionManager` and `SessionRoomRegistry` as struct fields and dispatches
through twelve more daemon-owned modules — and two of its edges close **mutual pairs** that Cargo
cannot express across a crate boundary:

    session_list_enrichment.rs:305     → elicitation::pending_elicitation_for_session_dir
    telegram_session_control.rs:33,673 → session_list_enrichment::{…}

    session_notifications.rs:366       → elicitation::mode_changed_requires_telegram_elicitation
    telegram_session_subscriber.rs:14  → session_notifications::{…}

Cutting *through* the subsystem is what made four modules movable at all: `elicitation` moved and its
two daemon callers stayed, giving a one-way `tddy-daemon` → `tddy-telegram` edge, while the two
modules holding the reverse edges stayed with those callers so their edges are intra-daemon.
**Moving all nine at once, as planned, is the one arrangement that cannot compile.**

Consequences worth carrying forward: `teloxide` does **not** leave `tddy-daemon`, no dedicated
telegram test file moved (all six also touch deferred modules), and **the telegram remainder is a
successor of nodes 4 and 6–8, not a peer of nodes 3–5.** A `TODO(unbundle-node-2, M4)` in
`packages/tddy-telegram/src/lib.rs` names the five blocked files.

## The published stub surface was fiction, and the code won

Both `tddy-telegram/src/lib.rs` and `tddy-screen-sharing/src/lib.rs` were published in the draft-PR
contract before the subsystems were read, and each contradicted the code in four places:
`TelegramError` and `ScreenSharingError::Unsealable` had **no raiser anywhere**, `LifecycleEvent`
could not carry the instance id the real message contains, `send_daemon_lifecycle_message` had no
config parameter and so could express neither the enabled-gate nor the per-chat fan-out, and
`build_screen_sharing_entry` took a vault that is per-session and on-disk and is never injected.

Every one was resolved **toward the code**. Inventing an error type with no raiser would have been
new behaviour inside a relocation. The two failing stub tests were restated against the real
behaviour keeping their intent, and a third was added for `enabled: false` — the state an operator
uses to mute the bot without deleting their token, which the stub surface could not express.

## The caller diff is zero, and that is the point

`packages/tddy-daemon/src/lib.rs` trades each `pub mod <m>;` for `pub use tddy_telegram::<m>;`, the
same facade mechanism node 1 used for `config` and the worktree modules. Six daemon source files and
six daemon test files reference the four moved telegram modules and **not one changed**. The whole
diff is 46 files, +1,062/−1,542 under `packages/`, and inside every moved file the changed lines are
`use` re-points and doc links — git scores the moves 96–100% similar.

## File budget: one over, recorded rather than forced

`screen_sharing_service.rs` landed at **2,171 lines and was not split**. 1,209 of those are the
`#[cfg(all(test, unix))]` host-scope suite, so the production file is 962 lines. Splitting it would
mean either cutting the seam between session-scoped and host-scoped calls — a design change
`#hosts-screen 8/8` deliberately avoided so the bridge and the LiveKit republishing stay shared — or
lifting the inline suite into `tests/`, which would cost it the private helpers it asserts on.

## Log targets were deliberately not renamed

44 `log::` calls in the moved code still name `tddy_daemon::…`, and one is a public constant
(`TELEGRAM_INBOUND_MESSAGE_BODY_LOG_TARGET = "tddy_daemon::telegram_bot::message-body"`) that
operators match on in `daemon.yaml` `log:` policies. **A log target is an operator-facing interface,
not a path**: renaming one silently breaks a deployed filter, so that is a deliberate step with a
migration note, not part of a relocation.

## Baseline

CI is the authority for whole-workspace health, and it is green on this branch:
**6463/6463 Rust tests, 2630/2630 Web**, plus Rust build, arm64 build, workspace clippy, the
generated-code drift gate and VM checks. Locally the run was scoped to the packages this node
touches, per [the verification policy](../guides/ci.md).

New tests this node adds: 3 crate-level in `tddy-model-registry`, 3 in `tddy-telegram`, 4 in
`tddy-screen-sharing`, 3 asserting the VNC files are gone, and 4 asserting the daemon's live service
roster. The 226-test question node 1 answered applies here too — **no test was lost to the split**:
the moved suites run under their new crates and the daemon's own suite shrank by exactly the files
that left.

## Open items

Recorded in `docs/dev/todo/`:
[`vnc.proto` has no server and a live web client](../todo/2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md),
[`StubEligibleDaemonSource` re-read](../todo/2026-09-06-stubeligibledaemonsource-is-no-longer-reachable-from-production.md)
(still open, unchanged by this node).

Inherited and deliberately not fixed here: `tddy-service` depends on `tddy-tui`, so every new
subsystem crate pulls the TUI into its build; the 44 `tddy_daemon::…` log targets; and the open
items already recorded inside `model_registry/` (`2026-08-16-models-agents-*`), which survive the
move unchanged — a reviewer seeing them in the new crate should read them as inherited, not
introduced.

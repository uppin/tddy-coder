# Changeset: carve-telegram

**Date**: 2026-09-15
**Status**: 🚧 In Progress
**Type**: Refactor + Architecture Change
**Stack**: `#carve` 7/9

PRD: [`2026-09-15-carve-telegram-prd.md`](./2026-09-15-carve-telegram-prd.md)

## Initial Discovery

[`2026-09-15-carve-telegram-initial-discovery.md`](./2026-09-15-carve-telegram-initial-discovery.md)

## Affected Packages

- **`tddy-telegram-control`** (new): the six Telegram modules, 7,403 lines.
- **`tddy-session-lifecycle`**: [README.md](../../../packages/tddy-session-lifecycle/README.md) —
  loses 19% of itself and its only `teloxide` dependency.
- **`tddy-daemon-kernel`**: gains the `PresenterObserverSpawner` port.
- **`tddy-daemon`**: `runtime.rs` injects the adapter; `lib.rs` facade re-points.

## Responsibility

- Replace `ConnectionServiceImpl`'s `telegram: Option<Arc<TelegramDaemonHooks>>` with a
  `tddy-daemon-kernel` port, injected by `tddy-daemon`'s `runtime.rs`.
- Split `telegram_session_control.rs` (3,980 prod, one 2,634-line `impl`) into seven modules.
- Move all six Telegram modules into a new `tddy-telegram-control`.
- Re-point `tddy-daemon`'s facade so its 12 Telegram test suites are not edited.

## Boundaries

- Does **not** test or decompose `telegram_callback_handler` (CRAP 88) or
  `telegram_message_handler` (CRAP 52). They move **unchanged and still untested**; the backlog entry
  recording them stays open, annotated with their new home.
- Does **not** touch `connection_service/`'s other 64 files.
- Does **not** change Telegram command syntax, callback payloads or message formatting.
- Does **not** move the control modules into the existing `tddy-telegram` — that would close a cycle.
- Does **not** edit the daemon's Telegram test suites.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `1/9` restructure-moves | facade-aware cycle refusal; nested anchors | the moves leave `pub use` facades in two crates | touch `tddy-code-restructuring` |
| `3/9` restructure-clusters | **multi-module cluster moves** | `telegram_notifier ↔ telegram_session_control` and `telegram_notifier ↔ telegram_multi_select_shortcuts` are **mutual**. This is the one node in the stack that genuinely needs it | rely on leaf-first ordering |
| `4/9`, `5/9`, `6/9` | `tddy-core` changes | **not consumed** | touch `tddy-core` |

## Draft PR contract

Published first:

1. `PresenterObserverSpawner` in `tddy-daemon-kernel`, with its real signature — the port
   `connection_service` holds and `tddy-daemon` injects.
2. The seven `telegram_session_control/` module files with their facades.
3. Failing tests pinning AC1–AC6, including AC2's Telegram-disabled start.

This PR goes on to implement all of it. **It must not merge in that state.**

## Green wave

**Wave:** 3 of 4
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

Phase A runs **before** Phase B deliberately: the port change touches
`svc_resolve_tddy_tools_path.rs` and `connection_service.rs`, and doing it after the 3,980-line file
has been carved keeps the two diffs separable for review.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset — this document
- [ ] Publish the draft-PR contract (port signature + module shape + failing tests)
- [ ] Failing acceptance tests — **USER REVIEW**
- [ ] Failing unit/integration tests
- [ ] Implement production code making tests pass (`/green`)
- [ ] Annotate the CRAP backlog entry with the handlers' new crate
- [ ] `/validate-changes`
- [ ] `/pr-wrap` — correct the title, ready for review
- [ ] Add a changeset entry under `docs/dev/changesets/` (`/wrap-context-docs`)

## Verification

```bash
./test -p tddy-session-lifecycle -p tddy-telegram-control -p tddy-daemon-kernel
cargo clippy -p tddy-session-lifecycle -p tddy-telegram-control -- -D warnings
cargo build -p tddy-daemon        # proves AC6 — the facade re-point compiles its 139 test binaries' paths
cargo fmt --all --check
```

The daemon's 12 Telegram acceptance suites (4,901 lines) are the real regression gate here, and they
reach the cluster through `tddy_daemon::telegram_*`. They must pass **unedited**.

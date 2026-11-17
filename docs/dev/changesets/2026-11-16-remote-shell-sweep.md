# 2026-11-16 — The files and functions the #remote-shell stack grew are split back under their budgets

**Type:** Refactor

The `#remote-shell` sweep (`#remote-shell` 8/11, PR #924): every code-issue record the stack
added or made worse is closed, or brought back to its measurement before the stack. Nothing
changes behaviour. Every moved Rust item was moved by `tddy-tools restructure`; the one
TypeScript file was split by hand under its existing specs; every path another crate or a test
names stays reachable.

## What landed

- `tddy-agent-launch`: `remote_agent_launch.rs` is a module with `startup` and `target`
  submodules; methods were extracted from the harness start and resume functions.
- `tddy-session-lifecycle`: `svc_activity_ports.rs`, `svc_host_builders.rs` and
  `connection_service.rs` each lost a group of items to a submodule.
- `tddy-accounts`: `service.rs` lost its link-progress, link/account-answer, route and wiring
  parts to child modules.
- `tddy-worktree-service`: `remote_git_service.rs` lost request resolution and the child relay
  to `request.rs`, `child_relay.rs` and `pumps.rs`.
- `tddy-daemon`: `build_session_host` lost the Coder credential wiring to a helper.
- `tddy-session-create-web`: `useCreateSessionPaneState.tsx` lost the derived-host resolution
  to `useDerivedHostSelection.ts`.
- Test support: the shared stub writers in `tddy-session-lifecycle`'s suites pay the
  first-execution scan at fixture build (`TDDY_FIXTURE_WARMUP`), so no timed launch pays it.
- One flaky test fixed in the test where the race lived: the daemon clone-restore test asserted
  the divergence report the instant the restored file was visible, although the report reaches
  daemon A afterwards; it now waits for the report itself under the same 30 s bound.

## Final measurements (production lines; functions brace-matched)

Closed, records deleted with this entry as their audit trail:

| # | File or function | Before the stack | At wrap |
|---|---|---|---|
| 1 | `tddy-agent-launch/src/remote_agent_launch.rs` | new (579 at stack top) | 338 (`startup.rs` 145, `target.rs` 116) |
| 2 | `svc_activity_ports.rs` | 513 | 412 |
| 3 | `svc_host_builders.rs` | 531 | 470 |
| 5 | `tddy-accounts/src/service.rs` | 772 | 421 |
| 6 | `remote_git_service.rs` | 1,004 | 418 |
| 7 | `useCreateSessionPaneState.tsx` | 495 | 498 |

Restored to at most their before-the-stack length, records kept and narrowed:

| # | Function | Before the stack | At wrap |
|---|---|---|---|
| 4 | `connection_service.rs` (whole file) | 558 | 556 (431 by `restructure lines`) |
| 10 | `resume_cli_harness_session` | 136 | 135 |
| 11 | `resume_cli_harness` | 88 | 83 |
| 12 | `build_session_host` | 73 | 73 |
| 13 | `start_cli_harness_from_request` | 94 | 84 |
| 14 | `start_sandboxed_cli_harness_session` | 291 | 287 |
| 15 | `start_session_on_request` | 79 | 73 |

Not restored, left exactly as they are, recorded in the changeset's deferred work for the
stack's TODO gate (no `docs/dev/todo/` entry created): row 9, `start_cli_harness_session`
(368 against 318 — the engine refuses the seam, and two hand-written `use` lines need the
developer's consent), and row 16, `DaemonSessionHost::new` (100 against 97 — no
extract-into-function assist for a `Self { … }` literal). Row 8, the generated `host_pb.ts`,
has no seam and was left.

## Backlog entries resolved here

- `2026-10-21-tddy-accounts-service-rs-over-file-budget.md` — the `service.rs` split it
  designed is applied (421 production lines).
- `2026-11-16-shared-stub-writers-fixture-warmup.md` — the warm-up is added to both shared
  stub writers.

Both files are deleted with this wrap; this entry is their audit trail.

## Verification (scoped, local)

`CARGO_INCREMENTAL=0`, failing sets held against the recorded baseline by name:
`tddy-agent-launch` 342/0, `tddy-accounts` 152/0, `tddy-worktree-service` 175/0/1 ignored,
`tddy-session-lifecycle` 966 passed with exactly the baseline's 13 local-only failures
(12 BSD `cat`, 1 `/tmp` path), `tddy-daemon` 334/0/1 ignored (the baseline's two LiveKit
port-bind failures did not recur; which test that bind hits varies per run),
`tddy-session-create-web` 104 unit tests passed and typecheck unchanged (252 errors, all
under `cypress/`). `cargo fmt --check` clean; `cargo clippy` on the five Rust packages with
`-D warnings` clean. Whole-workspace health is CI's.

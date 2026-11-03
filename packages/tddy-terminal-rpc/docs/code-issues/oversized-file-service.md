# oversized-file: service.rs

**Location:** `packages/tddy-terminal-rpc/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-11-02 — `/pr-wrap` step 3.5 on PR #805 (`#session-tabs` 2/7)
**Metrics:** **546 production lines** (2026-11-02, after #806; lines before the first `#[cfg(test)]`) · budget 500 · **1.09× over**
**Thresholds breached:** length 546 > 500
**Restructure:** `extract_module --to_file` — `/code-restructuring` territory
**Status:** Open — regressed 2026-11-02 (522 → 546 in #806) — **unclaimed**. Not split in #805 or #806: the file is stack-shared — `#session-tabs` 3/7 (#806), 5/7 (#808) and 7/7 (#810) modify it, and a split in any one node would conflict in each of the others
**Deferred:** by the developer 2026-11-03 (#session-tabs sweep: code issues deferred, logic issues solved)

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-02 | 522 | first detection; 500 → 522 in #805: `get_terminal_history` asks the store whether the terminal is running and serves a stopped one from `recorded_history` (the serving itself is in `service/recorded_history.rs`) |
| 2026-11-02 | 546 | 522 → 546 in #806 (`#session-tabs` 3/7): the `list_tmux_sessions` handler (a pass-through to the `TmuxHost` port), `from_request`'s new `?` and `os_user` on the launch, `tmux_session` on each listed terminal, and the wider proto import |
| 2026-11-03 | 546 | **unchanged** through #807–#810 (`#session-tabs` 4/7–7/7, merged): 546 lines before the first `#[cfg(test)]` at the merge-base `e9d036e0c` (500) and at master `162a66978` (546). `tddy-tools analyze` (88 tests, join 64.9%): `relay_control_changes` complexity 4, `controlled_terminal` 3, `serve_recorded_history` 3 — none over the threshold. Still open, unclaimed; the split the record names is no longer blocked by an in-flight stack |

## What the tool found

`TerminalSessionServiceImpl` holds every handler of `terminal_session.TerminalSessionService` — the
four I/O methods, the three lifecycle methods, `ListTmuxSessions` and the two control methods — plus their shared helpers
and `build_terminal_session_entry`. The port types already live in `service/ports.rs`.

## What would close it

Move the lifecycle handlers' bodies (`start_terminal_session`, `stop_terminal_session`,
`list_terminal_sessions`, `list_tmux_sessions`) or the control pair into a sibling module under `service/`, the way
`recorded_history.rs` holds the stopped-terminal history, leaving the trait impl as thin delegation.
Do it after the `#session-tabs` stack lands, with the seam proven by `tddy-tools restructure`.

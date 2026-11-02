# oversized-file: service.rs

**Location:** `packages/tddy-terminal-rpc/src/service.rs`
**Category:** oversized-file
**Detected:** 2026-11-02 — `/pr-wrap` step 3.5 on PR #805 (`#session-tabs` 2/7)
**Metrics:** **522 production lines** (2026-11-02; lines before the first `#[cfg(test)]`) · budget 500 · **1.04× over**
**Thresholds breached:** length 522 > 500
**Restructure:** `extract_module --to_file` — `/code-restructuring` territory
**Status:** Open — **unclaimed**. Not split in #805: the file is stack-shared — `#session-tabs` 3/7 (#806) and 7/7 (#810) modify it too, and a split here would conflict in each

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-02 | 522 | first detection; 500 → 522 in #805: `get_terminal_history` asks the store whether the terminal is running and serves a stopped one from `recorded_history` (the serving itself is in `service/recorded_history.rs`) |

## What the tool found

`TerminalSessionServiceImpl` holds every handler of `terminal_session.TerminalSessionService` — the
four I/O methods, the three lifecycle methods and the two control methods — plus their shared helpers
and `build_terminal_session_entry`. The port types already live in `service/ports.rs`.

## What would close it

Move the lifecycle handlers' bodies (`start_terminal_session`, `stop_terminal_session`,
`list_terminal_sessions`) or the control pair into a sibling module under `service/`, the way
`recorded_history.rs` holds the stopped-terminal history, leaving the trait impl as thin delegation.
Do it after the `#session-tabs` stack lands, with the seam proven by `tddy-tools restructure`.

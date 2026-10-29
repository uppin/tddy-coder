# oversized-file: exec_tool/project_home.rs

**Location:** `packages/tddy-daemon-rpc/src/exec_tool/project_home.rs`
**Category:** oversized-file
**Detected:** 2026-10-25 — `/pr-wrap` step 3.5 file-length gate on #753 (`#start-parity` 4/9), production lines counted to the first `#[cfg(test)]`
**Metrics:** **830 production lines** (2026-10-29) · budget 500 · 1.3× over
**Thresholds breached:** length 641 > 500
**Restructure:** not designed
**Status:** Open — partially fixed (641 > 500: seams B and C remain); split deferred: the file is stack-shared, the follow-up is after the stack lands — **unclaimed**
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured; the seams are a first reading

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-10-25 | 665 | first detection. `#start-parity` 4/9 (#753): 478 → 665 (+187) — `start_ledger_of` with its once-per-process recovery, the `start_session` / `get_session_start` / `cancel_session_start` port methods (local and forwarded cancel), and `StartedSessionListings` (the started session's branch and worktree, local or from a peer). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 708 | **regressed.** `#start-parity` 5/9 (#754): 665 → 708 (+43) — `DaemonProjectHome`'s catalogue, project, host-service and provider-account sources and `ProjectHomePort::start_options`; the answer itself is the new `exec_tool/project_home_start_options.rs` (282). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 730 | **regressed.** `#start-parity` 6/9 (#755): 708 → 730 (+22) — `DaemonProjectHome`'s `session_activity`, `session_transcript` and `session_tool_call` port methods, each delegating; the reads themselves are the new `exec_tool/project_home_session_follow.rs` (121). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-25 | 746 | **regressed.** `#start-parity` 7/9 (#756): 730 → 746 (+16) — `DaemonProjectHome::terminate_session`, delegating to `tddy_agent_launch::svc_signal_delete_session::signal_session_dir` (the `SignalSession` body, now shared). **Split deferred by developer instruction** (stack-shared) |
| 2026-10-26 | 788 | **regressed.** `#start-parity` 8/9 (#757): 746 → 788 (+42) — `DaemonProjectHome::refresh_worktree_disk_usage` (membership check, enqueue through `WorktreeSizeCalculator`) and its imports. **Split deferred by developer instruction** (stack-shared) |
| 2026-10-26 | 641 | **better, not clean.** `#start-parity` 9/9 (#758): 788 at its base (`#start-parity` 8/9 tip) → 641 (−147) — the start's begin-and-record, its cancel and `StartedSessionListings` moved to `tddy-agent-launch/src/conversation_start.rs` (`LaunchSessions::begin_recorded_start` / `cancel_recorded_start`, shared with the conversation-spawn handler), and the ledger's once-per-process recovery to `StartLedger::recovered_in_session_dir`; `project_home_caller` admits a conversation spawner for the four start tools. Seam A is done. **Split deferred by developer instruction** (stack-shared) |
| 2026-10-28 | 686 | **regressed.** `#home-inbox` 2/8 (#767): 641 → 686 (+45) — `DaemonProjectHome::terminals`, the dispatch of `ReadSessionTerminal` and its `ProjectHomeTerminalPort::read_terminal` impl (store lookup, capture lock, `text_grid`). **Split deferred** (stack-shared: `#home-inbox` 5/8 and 6/8 touch the file); the port impl is the first candidate to move to its own `exec_tool/project_home_terminal.rs` with them |
| 2026-10-28 | 779 | **regressed.** `#home-inbox` 5/8 (#770): 686 → 779 (+93) — `DaemonProjectHome`'s caller identity, terminal control and lease store fields, `claim_key`, the `claim_terminal` and `send_keys` port impls and `control_refusal`. **Split deferred** (stack-shared: `#home-inbox` 6/8–8/8 touch the file); the terminal port impl (read, claim, send) is the first candidate for `exec_tool/project_home_terminal.rs` |
| 2026-10-29 | 830 | **regressed.** `#home-inbox` 6/8 (#771): 779 → 830 (+51) — the `pending_answers` field on `DaemonProjectHome`, the `AnswerSessionPermission` dispatch arm and the `ProjectHomeAnswerPort::answer_permission` impl (delegating to `tddy_session_activity::answers::answer_permission`). **Split deferred** (stack-shared: `#home-inbox` 7/8 and 8/8 touch the file); the answer port impl moves with the terminal ports to a sibling module |

## What would close it — candidate seams (not proven)

| Seam | Items |
|---|---|
| B | the session listing (`rows`, `session_rows`, `list_sessions`, `list_peer_sessions`) |
| C | the caller resolution and tool-call log (`project_home_caller`, `record_call`) |

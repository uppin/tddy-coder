# complexity: run_sandbox_runner_inner

**Location:** `packages/tddy-sandbox-runner/src/runner.rs:2475` — `run_sandbox_runner_inner`
**Category:** complexity
**Detected:** 2026-11-02 — `/pr-wrap` step 4 (`/analyze-clean-code`) on PR #794 (`#agent-tools` 5/12)
**Metrics:** **261 lines** (245 at the base) · budget 60; `run_generic_pty_thread` beside it 126 lines (119 at the base)
**Thresholds breached:** length 261 > 60
**Restructure:** `extract_method` — `/code-restructuring` territory; the file is stack-shared within `#agent-tools`
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02) — **unclaimed**

## Measurement history

| Run | Metric | Note |
|---|---|---|
| 2026-11-02 | 261 lines | first detection (245 at the base `eae01cf5c`) |

## What the tool found

The runner's shared body: boot, egress shim, then a four-way choice of PTY (exec plan, generic, opencode/muse, claude), then the gRPC server on UDS or loopback. PR #794 added the exec-plan arm (+16) and `ChildEnv` on the generic PTY thread (+7); it also folded the duplicated exit-notify block into `notify_when_the_process_exits`. See `oversized-file-runner.md` for the file.

## What would close it

Extract the PTY choice into `spawn_session_pty(args, exec_plan, …) -> PtyState`, and the two server tails into one function over a transport. Do it after the stack lands: every later `#agent-tools` node edits this file.

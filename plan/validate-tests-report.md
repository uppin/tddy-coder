# Validate Tests Report — web-dev daemon-only refactor

## Executive summary

Web-dev–scoped tests (`web_dev` filter) **all passed** (6 tests: 3 unit + 3 integration). The **full** `tddy-e2e` package run **failed once**: `grpc_reconnect_acceptance::grpc_reconnect_second_stream_receives_full_tui_render` did not match expected TUI screen content after reconnect (OAuth select highlight). All other `tddy-e2e` binaries in this run passed; one test is **ignored** (`pty_clarification`). This failure is **not** in the web-dev script/contract tests and aligns with evaluation-report notes on unrelated gRPC/TUI surface area. Exit code for the full run was **101** (Cargo reports one failed test target).

## Commands run + exit codes

| # | Command | Working directory | Exit code |
|---|---------|-------------------|-----------|
| 1 | `./dev cargo test -p tddy-e2e web_dev --no-fail-fast -- --test-threads=1` | `/var/tddy/Code/tddy-coder/.worktrees/web-dev-daemon-only` | **0** |
| 2 | `./dev cargo test -p tddy-e2e --no-fail-fast -- --test-threads=1` | `/var/tddy/Code/tddy-coder/.worktrees/web-dev-daemon-only` | **101** |

`./dev` printed a dirty-git-tree warning and the usual dev-shell banner; no separate stderr-only failure stream was captured beyond Cargo’s combined output.

## Pass/fail table (per test binary / suite)

### Filtered run: `web_dev` (command 1)

| Binary / target | Passed | Failed | Ignored | Notes |
|-----------------|--------|--------|---------|--------|
| `tddy_e2e` (lib unittests) | 3 | 0 | 0 | `web_dev_contract::granular_tests::*` |
| `grpc_clarification` | 0 | 0 | 0 | All tests filtered out |
| `grpc_full_workflow` | 0 | 0 | 0 | Filtered out |
| `grpc_reconnect_acceptance` | 0 | 0 | 0 | Filtered out |
| `grpc_terminal_rpc` | 0 | 0 | 0 | Filtered out |
| `livekit_terminal_rpc` | 0 | 0 | 0 | Filtered out |
| `pty_clarification` | 0 | 0 | 0 | Filtered out |
| `pty_full_workflow` | 0 | 0 | 0 | Filtered out |
| `rpc_frontend_resize` | 0 | 0 | 0 | Filtered out |
| `terminal_service_livekit` | 0 | 0 | 0 | Filtered out |
| `token_generation_livekit` | 0 | 0 | 0 | Filtered out |
| `token_service_livekit` | 0 | 0 | 0 | Filtered out |
| `virtual_tui_sessions` | 0 | 0 | 0 | Filtered out |
| `web_dev_script` | 3 | 0 | 0 | All `web_dev_*` tests ran |

### Full package run: `-p tddy-e2e` (command 2)

| Binary / target | Passed | Failed | Ignored | Result |
|-----------------|--------|--------|---------|--------|
| `tddy_e2e` (lib) | 3 | 0 | 0 | **PASS** |
| `grpc_clarification` | 1 | 0 | 0 | **PASS** |
| `grpc_full_workflow` | 2 | 0 | 0 | **PASS** |
| `grpc_reconnect_acceptance` | 0 | **1** | 0 | **FAIL** — see below |
| `grpc_terminal_rpc` | 8 | 0 | 0 | **PASS** |
| `livekit_terminal_rpc` | 1 | 0 | 0 | **PASS** (skipped stub) |
| `pty_clarification` | 0 | 0 | 1 | **PASS** (suite ok; test ignored) |
| `pty_full_workflow` | 1 | 0 | 0 | **PASS** |
| `rpc_frontend_resize` | 2 | 0 | 0 | **PASS** |
| `terminal_service_livekit` | 1 | 0 | 0 | **PASS** (skipped stub) |
| `token_generation_livekit` | 1 | 0 | 0 | **PASS** (skipped stub) |
| `token_service_livekit` | 1 | 0 | 0 | **PASS** (skipped stub) |
| `virtual_tui_sessions` | 1 | 0 | 0 | **PASS** |
| `web_dev_script` | 3 | 0 | 0 | **PASS** |
| Doc-tests `tddy_e2e` | 0 | 0 | 0 | **PASS** |

**Failure detail** (`grpc_reconnect_acceptance`, test `grpc_reconnect_second_stream_receives_full_tui_render`):

- Panic location: `packages/tddy-e2e/tests/grpc_reconnect_acceptance.rs:141`
- Message (tail): assertion that reconnect must preserve Select highlight on OAuth; actual screen showed planning state and OAuth options with `> Email/password` highlighted, framed as mismatch vs expected “full TUI render” / highlight preservation after reconnect.

```text
Reconnect must preserve Select highlight on OAuth (view-local state + presenter sync). Got screen:
State: Init → Planning                                                        -
Agent exited (code 0) for plan
...
[1] Scope: Which authentication method do you want?
> Email/password -- Traditional login
  OAuth -- Social login
...
```

Cargo ended with: `error: 1 target failed: -p tddy-e2e --test grpc_reconnect_acceptance`.

## Coverage gaps / risks

1. **No runtime E2E for web-dev + Vite + daemon** — Matches `/var/tddy/Code/tddy-coder/.worktrees/web-dev-daemon-only/plan/evaluation-report.md`: contract tests are static (shell/syntax, file content, default paths); starting the real stack is manual or out of scope here.
2. **Contract tests are substring/static** — `web_dev_contract` and script tests can be brittle if comments or unrelated files introduce legacy tokens; evaluation-report already flags this.
3. **`grpc_reconnect_acceptance` failed in full run** — Suggests possible flake or real regression in reconnect + Select/OAuth TUI state; **not** introduced by web-dev shell changes but **blocks a green full `tddy-e2e` run** until fixed or quarantined per project policy.
4. **LiveKit-related binaries** — Several tests are explicit skips (`*_skipped`); coverage of real LiveKit paths is not exercised in this run.
5. **PTY clarification** — One test ignored unless built with demo binary; reduces confidence in that path in default CI.

## Recommendations

1. **Treat `grpc_reconnect_second_stream_receives_full_tui_render` as blocking** for claiming “full tddy-e2e green”: investigate whether the assertion is too strict, timing-related, or indicates a real reconnect/state bug; re-run the single test to check flakiness (`./dev cargo test -p tddy-e2e --test grpc_reconnect_acceptance -- --test-threads=1 --nocapture`).
2. **Keep web-dev validation** on the `web_dev` filter + `web_dev_script` + `web_dev_contract` tests for PRs that only touch `web-dev` and daemon config docs.
3. **Document or schedule** a manual/automated smoke for `./web-dev` with real `tddy-daemon` + Vite if product risk warrants it (per evaluation-report “runtime E2E remains manual”).
4. **Optional**: Narrow `web_dev_contract` matchers over time to reduce false positives from comments (evaluation-report suggestion).

---

*Report generated from command output on this workspace; paths are absolute as requested.*

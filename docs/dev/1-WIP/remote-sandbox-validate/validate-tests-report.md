# Validate Tests Report — Remote Sandbox

## Executive summary

The full workspace test suite was run from the repository root using `./test` (project convention: nix dev shell, `cargo build` prerequisites, then `cargo test --workspace` with `--test-threads=1`, output tee’d to `.verify-result.txt`). **All tests passed** (command exit status **0**). Runtime was approximately **11.5 minutes** (~689s), including a cold compile.

Remote-sandbox–related packages exercised successfully:

| Area | Result |
|------|--------|
| `tddy-daemon` — `remote_sandbox_connect_integration`, `remote_sandbox_vfs_rules` | Pass (6 tests) |
| `tddy-integration-tests` — `remote_sandbox_livekit` | Pass (`livekit_exec_smoke`) |
| `tddy-remote` — integration tests | Pass (7 tests across guards/CLI/config/shell/VFS) |
| `tddy-service` — `remote_sandbox_stub_red` | Pass (4 tests) |

The evaluation report noted **cargo check** only for touched crates; this run confirms **automated tests** also pass for those areas. Items it flagged as non-test (security review, hygiene file, codegen warnings) remain **outside** what `cargo test` validates.

## Commands run

```bash
cd /var/tddy/Code/tddy-coder/.worktrees/remote-sandbox-grpc-livekit
./test
```

Equivalent inner behavior (from `./test`):

- `nix develop --profile ./.nix-profile -c …`
- `cargo build -p tddy-coder -p tddy-tools -p tddy-livekit -p tddy-acp-stub --examples --bins`
- `cargo test "$@" -- --test-threads=1` with output `tee` to **`.verify-result.txt`**

No fallback to `./verify` or bare `cargo test --workspace` was required.

## Overall pass/fail

- **Pass/fail:** **PASS**
- **Exit status of test command:** **0**

## Per-area notes

### `tddy-daemon`

- **`tests/remote_sandbox_connect_integration.rs`** — 4 tests, **0 ignored**: `concurrent_sandboxes_isolated`, `shell_remote_exit_code_propagation_via_connect`, `vfs_rsync_push_checksum`, `vfs_rsync_pull_checksum` (~0.9s). Uses `CARGO_BIN_EXE_tddy-daemon` / `CARGO_BIN_EXE_tddy-remote`, sets `TDDY_REMOTE_RSYNC_SESSION` and `RSYNC_RSH` for rsync-style flows (not an env *gate* that skips tests; required for those scenarios).
- **`tests/remote_sandbox_vfs_rules.rs`** — 2 tests: path acceptance and `..` rejection.

### `tddy-service`

- **`tests/remote_sandbox_stub_red.rs`** — 4 tests (`red_exec_*`, `red_put_object_succeeds`, `red_stat_object_succeeds`), all passed.
- Build emitted **warnings** from generated `remote_sandbox.v1.rs` (unused imports, unused `method` in generated code) — aligns with evaluation-report “codegen warnings”; tests still pass.

### `tddy-remote`

- **`tests/cargo_graph_guard.rs`** — dependency guard (`cargo_graph_no_tddy_workflow_in_remote_crates`).
- **`tests/cli_list.rs`**, **`config_yaml_parse.rs`**, **`vfs_path_normalize.rs`** — CLI/config/VFS normalization.
- **`tests/shell_exit_code.rs`** — 2 tests (`shell_pty_bash_smoke`, `shell_remote_exit_code_propagation`).

### `tddy-integration-tests`

- **`tests/remote_sandbox_livekit.rs`** — single async test `livekit_exec_smoke` (~4.85s), **0 ignored**. Uses `LiveKitTestkit::start()` from `tddy-livekit-testkit`. Per testkit docs, if **`LIVEKIT_TESTKIT_WS_URL`** is set, the same API **reuses** an existing server instead of managing container lifecycle; default CI path starts/spins testkit as needed. Module comment documents this optional reuse.

### Workspace-wide ignored tests (not remote-sandbox-specific)

Examples from the same run (for transparency; **not** failures):

- Doc-test: `tddy-livekit` — `rpc_log` doc example **ignored** (1 ignored doc-test).
- Other packages: PTY/manual tests (e.g. `clarification_question_appears_on_screen` with `--ignored`), workflow/engine **ignored** scenarios, etc.

None of the **remote sandbox** test binaries reported ignored tests.

## Failed tests

**None.** No `FAILED` or failing `test result` lines for remote-sandbox or workspace run.

## Coverage gaps vs evaluation report

| Evaluation item | Test coverage today |
|-----------------|---------------------|
| PRD intent (Connect, LiveKit RPCs, daemon wiring, `tddy-remote`, VFS, rsync bridge) | Strong automated coverage via daemon integration, LiveKit smoke, service stubs, remote CLI tests. |
| **Medium/security — RPC attack surface / auth** | **Not covered** by `cargo test`; remains manual review / future auth tests. |
| Codegen warnings on `remote_sandbox.v1.rs` | Observed at compile; **not** a test failure. |
| `default_authority` unread in `tddy-remote` `config.rs` | **Dead_code warning**; behavior not asserted by tests. |
| `.red-remote-sandbox-test-output.txt` hygiene | **Not** a test; file should be removed or gitignored per evaluation. |
| `LIVEKIT_TESTKIT_WS_URL` **reuse** path | Optional optimization path; default run exercised **container/testkit start**. CI could additionally run with env set to validate reuse (same binary, different env). |
| **Branch behind origin/master** | Git state; unrelated to test pass/fail. |

**Integration gaps / manual-only paths**

- **Production auth** on remote sandbox RPCs: evaluation calls for review; no automated test asserts tenant/auth boundaries.
- **End-to-end** “real daemon + real network + production LiveKit” may differ from in-process stubs and testkit; current tests are layered (stub RED, testkit LiveKit, Connect integration with built binaries).

## Recommendations

1. **Keep `./test` (or CI equivalent)** as the gate before merge for this worktree; exit 0 with full workspace scope matches evaluation expectations beyond `cargo check`.
2. **Address codegen warnings** (prost/tonic generated `remote_sandbox.v1.rs` or build.rs attributes) to keep `-D warnings` CI viable if adopted.
3. **Security follow-up:** add explicit tests or threat-model doc for who may call remote sandbox RPCs once auth is defined (evaluation medium item).
4. **Optional:** periodic run with `LIVEKIT_TESTKIT_WS_URL` set (reuse server) to ensure the testkit “reuse” branch stays green without relying only on container startup.
5. **Hygiene:** remove or gitignore `.red-remote-sandbox-test-output.txt` per evaluation.

---

*Report generated for validate-tests subagent. Primary evidence: `./test` exit 0; detailed log in `.verify-result.txt` at repository root.*

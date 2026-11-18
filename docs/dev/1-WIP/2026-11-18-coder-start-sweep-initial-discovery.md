# Initial discovery: the coder-start sweep

**Date**: 2026-11-18 · **Tree**: `feature/coder-start/session-driver-cli` at `82fb5ad57` (measured on `6e88f90e6`; the one commit between is a test-only change in `tddy-livekit-compat-tests`)

## How the backlog was collected

- `git diff --name-status 2ed59a3c7..origin/feature/coder-start/session-driver-cli -- docs/dev/todo 'packages/*/docs/code-issues'` — no code-issue record added by any wrap; 8 todos added, 2 edited.
- `tddy-tools analyze coverage` + `report` per touched package. CRAP join: tddy-remote-shell 73.9% (184/249), tddy-tool-engine 68.1% (856/1257), tddy-agent-launch 65.3% (1340/2052), tddy-session-driver 67.6% (50/74). The report lists the top 50 functions per package.
- Production line counts before the first `#[cfg(test)]`, function lengths by brace match, against merge base `2ed59a3c7`.

## Findings in code the stack changed

| Finding | Evidence | In backlog |
|---|---|---|
| `coder.rs` 507 > 500 | `#[cfg(test)]` at 508; base 340 | yes |
| `launch_on_target` 184 > 60 | lines 163–346; base 180 — the stack added the `["SSH_AUTH_SOCK", "PATH"]` loop | yes (worsened) |
| `DriverSession::launch` 61 > 60 | lines 99–159; new package | yes |
| `DriverSession::launch`/`stop`/`wait_for` CRAP 56/56/42 | 0% coverage reported; `session_driver_acceptance` (7 tests) spawns `CARGO_BIN_EXE_tddy-session-driver`, which the capture does not instrument | no — tool blind spot |
| `DriverTarget::from_str` complexity 14 | CRAP 14.0, fully covered by `target.rs` unit tests | no |
| Stack-changed fns in `coder.rs` | `read_noting_the_marker` 5, `exec` 5, `stdin_lines`/`one_shot_command` 2; nothing above CRAP 8 in the package | no |
| tddy-tool-engine changed fns | `execute_project_home_terminal_tool`, `terminal_grid_answer` — not in the top 50 | no |
| `schema_offering` | 192 on base and head (one description string changed) | no — pre-existing record, not worsened |

## Seam checks

### `coder.rs` → `coder/one_shot.rs`

References to the moved items (line numbers on `82fb5ad57`):

| Item | Defined | Used at |
|---|---|---|
| `START_MARKER` | 85 | 90, 138, 157–159 (moved), 573 (test module) |
| `strip_start_marker` | 89 | 365, 372 (`CoderShell::output_of`) |
| `STDIN_LINE_WIDTH` | 104 | 117–118 (moved) |
| `STDIN_NOT_WHOLE_STATUS` | 107 | 134, 143, 149 (moved) |
| `stdin_lines` | 113 | 131 (doc), 414 (`exec`) |
| `one_shot_command` | 137 | 415 (`exec`) |
| `holds_the_start_marker_line` | 156 | 179 (moved) |
| `read_noting_the_marker` | 165 | 433–434 (`exec`) |

`CoderStdin` (189) stays: it belongs to `exec_streamed`, not the one-shot. `lib.rs` re-exports only
`CoderCredentials`, `CoderShell`, `CoderWorkspaceRef` from `coder`, none of which move.

### `launch_on_target` steps (lines on `82fb5ad57`)

1. pid directory: the `mkdir -p … && rm -f …` `exec` (~20 lines, `?` only).
2. launch files: `render_slots`/`slot_files`/`plan` bindings stay (read later); the two `for` loops writing slot files and session-dir files move (~30 lines, `?` only).
3. remote command: `env` + `agent_env` + `remote_agent_command` (~9 lines) → returns `command`.
4. client command: `interactive` → `argv` + `local_env` incl. the `SSH_AUTH_SOCK`/`PATH` loop (~30 lines) → returns `(argv, local_env)`.
5. start wait: `start_bound` + the `AgentStart` match — **stays** (value binding beside a `return Err` arm).
6. early exit: `if exits_during_startup(…) { …; return Err(…) }` (~22 lines) — a `return Err` guard run mid-function → lifted to `-> Result<(), RemoteLaunchError>`, called with `?`.
7. owner: `let owner = match remote.pointer { … }` (~23 lines, `?` inside) → returns `Result<Option<SessionPointerOwner>, RemoteLaunchError>`.

Projected: 184 − (20+30+9+30+22+23) + 6 calls ≈ 56.

### `DriverSession::launch`

`let deadline …` through the `if *handle.pty_done.borrow() { return Err(…) }` (11 lines) names no
`self` (it reads the local `spec`) and ends in a `return Err` guard → lifted; nothing after it reads
`deadline`. Projected 61 → ~51.

## TODOs the stack leaves (for the TODO gate, not this node)

None is closable by a restructure: each is deferred feature or behaviour work (target agent path,
folder-trust prompt, Cursor chat mint location, `exec_streamed` over the pty, one-shot deadline
cleanup, pagers, start visibility, idle-as-permission status) or test-infrastructure
consolidation across packages (`2026-11-16-remote-shell-test-fixtures-are-duplicated.md`).

## Revised at the contract commit

`check --deep` refused the lifted guards (steps 6 and the driver's `if`: rust-analyzer's shape for an
`async` guard is not one the lift knows) and the single range over both file loops (a dropped
comment it could not pair back). The checked design — a tail range from `let start_bound`, the owner
cut out of it in a second plan, the loops cut separately, the driver's wait loop alone — is in the
changeset's **Delta** and **Plan**; its projection is `launch_on_target` ~54, `DriverSession::launch` ~58.

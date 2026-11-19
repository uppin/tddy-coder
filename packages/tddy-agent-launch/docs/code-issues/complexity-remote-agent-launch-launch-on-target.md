# complexity: launch_on_target

**Location:** `packages/tddy-agent-launch/src/remote_agent_launch.rs:163` — `launch_on_target`
**Category:** complexity
**Detected:** 2026-11-18 — `/yolo-pr-stack` sweep-collect (`#coder-start`), function length by brace matching (fn line to closing brace)
**Metrics:** **60 lines** (was 184) · **6 parameters** · budget 60 lines, 5 parameters — length closed, parameter count still 6 > 5. CRAP: below the report's top-50 cut-off (CRAP < 42), join rate 65.3% for the package (1340 of 2052), so the function's own coverage figure was not read
**Thresholds breached:** parameters 6 > 5 (length closed by the #coder-start sweep)
**Restructure:** a parameter bundle (`HostAgent` + `TemplateParams` + `RenderedSlots` into one input) — changes the public signature, so a feature change, not a Refactor node
**Status:** Open — partially fixed (length closed in the sweep; parameter count remains) — **unclaimed**

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-18 (#947 wrap) | 59 → 60 | `pre_trust_a_claude_clone` call added before the terminal is registered; at the 60-line budget. Parameters unchanged at 6 (the clone root went on `RemoteLaunch`) |
| 2026-11-18 (sweep) | 184 → 59 | eight engine `extract_method`s: `judge_the_start`, `recorded_owner`, `client_command`, `remote_command`, `client_argv_and_env`, `plan_the_launch`, `write_slot_files`, `write_rendered_session_files`; every extracted function ≤ 33 lines. Parameters unchanged at 6 |
| 2026-11-18 | 180 → 184 | first detection as a record. 180 on `2ed59a3c7`, 184 on `6e88f90e6`: the ssh-only `SSH_AUTH_SOCK` copy became a `for name in ["SSH_AUTH_SOCK", "PATH"]` loop so a bare `coder`/`ssh` client is found on `PATH` (#942 area). One loop and its comment |

## What the tool found

One body creates the pid directory, builds the remote command, assembles the local PTY child's environment (`local_env`), launches the PTY, waits for the agent's start through `wait_for_agent_start`, records the owner, and registers the session. Not an analyzer score: a line and parameter count.

## Why it matters here

Every launch-path change on a remote target lands in this function; the stack's own `PATH` fix did. It is not tested in isolation, only through `remote_agent_launch_acceptance` and `coder_agent_launch_*`.

## What would close it

Bundle the six parameters (`agent`, `params`, `slots` describe one launch recipe; `remote`, `session` already bundle theirs) into one input type. That changes `launch_on_target`'s public signature and its callers, so it is not a restructure.

## Verified by hand

2026-11-18: counted 184 lines (163–346) and 180 on the base with the same awk-style brace match; read the diff — the stack's change is the loop and comment only.

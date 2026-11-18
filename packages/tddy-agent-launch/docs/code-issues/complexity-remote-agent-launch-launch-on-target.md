# complexity: launch_on_target

**Location:** `packages/tddy-agent-launch/src/remote_agent_launch.rs:163` — `launch_on_target`
**Category:** complexity
**Detected:** 2026-11-18 — `/yolo-pr-stack` sweep-collect (`#coder-start`), function length by brace matching (fn line to closing brace)
**Metrics:** **184 lines** · 6 parameters · budget 60 lines, 5 parameters · **~3× over**. CRAP: below the report's top-50 cut-off (CRAP < 42), join rate 65.3% for the package (1340 of 2052), so the function's own coverage figure was not read
**Thresholds breached:** length 184 > 60, parameters 6 > 5
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — length predates the `#coder-start` stack (180 on merge base `2ed59a3c7`); the stack added 4 — **unclaimed**

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-18 | 180 → 184 | first detection as a record. 180 on `2ed59a3c7`, 184 on `6e88f90e6`: the ssh-only `SSH_AUTH_SOCK` copy became a `for name in ["SSH_AUTH_SOCK", "PATH"]` loop so a bare `coder`/`ssh` client is found on `PATH` (#942 area). One loop and its comment |

## What the tool found

One body creates the pid directory, builds the remote command, assembles the local PTY child's environment (`local_env`), launches the PTY, waits for the agent's start through `wait_for_agent_start`, records the owner, and registers the session. Not an analyzer score: a line and parameter count.

## Why it matters here

Every launch-path change on a remote target lands in this function; the stack's own `PATH` fix did. It is not tested in isolation, only through `remote_agent_launch_acceptance` and `coder_agent_launch_*`.

## What would close it

Cut along the existing steps: the local PTY environment (`local_env` assembly, ~25 lines) into a function of the interactive command's env; the post-start owner recording and registration into a tail function. `RemoteLaunch`/`HostPtySession` already bundle most parameters.

## Verified by hand

2026-11-18: counted 184 lines (163–346) and 180 on the base with the same awk-style brace match; read the diff — the stack's change is the loop and comment only.

# heavy-params: dial_and_bridge

**Location:** `packages/tddy-daemon-sandbox/src/sandbox_session.rs` — `dial_and_bridge`
**Category:** heavy-params
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **12 parameters** — the highest in any package this PR touched · budget 5
**Thresholds breached:** parameters 12 > 5 (**2.4×**)
**Restructure:** an options struct
**Status:** Open — **unclaimed**
**Related:** `oversized-file-sandbox-session.md` (the file is 911 production lines)

## Measurement history

| Run | Params | Note |
|---|---|---|
| 2026-09-19 | 12 | first detection; signature unchanged by PR #518 |

## What the tool found

Twelve positional parameters on the function that dials a spawned jail and bridges its stdio to the
daemon. Several share a type, so a transposed pair compiles.

It is also the function that
[`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../../../../docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)
names for discarding the relay's `JoinHandle` (`run_host_relay_with_rpc(...).await?; Ok(())`), so a
reader arriving from that entry meets the parameter list first.

## What would close it

A `DialAndBridge { … }` struct. The neighbouring `SandboxRunnerSpawn` and `SandboxedCodebaseParams`
are the house pattern and both live in this crate's blast radius, so the shape is already agreed.

Rust has no `extract_type` in the restructure vocabulary, so this is a hand change under the usual
discipline — green baseline, mechanical move, same green after.

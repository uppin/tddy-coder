# heavy-params: dial_and_bridge

**Location:** `packages/tddy-daemon-sandbox/src/jail_host_bridge.rs` — `dial_and_bridge`
**Moved:** 2026-11-22 from `sandbox_session.rs` (`#tab-hardening` 3/4, PR #991, file-length split; re-exported from `sandbox_session`)
**Category:** heavy-params
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518
**Metrics:** **12 parameters** — the highest in any package this PR touched · budget 5
**Thresholds breached:** parameters 12 > 5 (**2.4×**)
**Restructure:** an options struct
**Status:** Open — **unclaimed**

## Measurement history

| Run | Params | Note |
|---|---|---|
| 2026-09-19 | 12 | first detection; signature unchanged by PR #518 |
| 2026-10-22 | 12 | unchanged; PR #670 adds a fourth caller (`svc_start_sandboxed_cli_harness_session`), so an options struct now pays off at four sites |
| 2026-10-25 | 12 | unchanged; PR #762 (`#term-snapshot` 3/4) carries the jail's size on the existing relay (`HostRelayConfig::with_terminal_events`) rather than a new parameter, and moves the terminal forwarder out of the body into `forward_jail_terminal_events` |
| 2026-11-02 | 12 (+ a 12-parameter sibling) | **worse** in PR #794 (`#agent-tools` 5/12): `dial_and_bridge_with_egress` takes the same eleven plus the jail's `EgressGuard`; `dial_and_bridge` forwards to it with an unrestricted guard. The `DialAndBridge` struct would now serve five call sites and both functions. deferred to the #agent-tools sweep node (developer, 2026-11-02) |
| 2026-11-22 | 12 | **unchanged** in #991 (`#tab-hardening` 3/4): `dial_and_bridge*` now return a `JailBridge` (a return type, no parameter). Moved with `sandbox_session.rs`'s bridge cluster to `jail_host_bridge.rs` |

## What the tool found

Twelve positional parameters on the function that dials a spawned jail and bridges its stdio to the
daemon. Several share a type, so a transposed pair compiles.

It is also the function that
[`docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`](../../../../docs/dev/todo/2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md)
names for discarding the relay's `JoinHandle`; since #991 it returns the handles as a `JailBridge`.

## What would close it

A `DialAndBridge { … }` struct. The neighbouring `SandboxRunnerSpawn` and `SandboxedCodebaseParams`
are the house pattern and both live in this crate's blast radius, so the shape is already agreed.

Rust has no `extract_type` in the restructure vocabulary, so this is a hand change under the usual
discipline — green baseline, mechanical move, same green after.

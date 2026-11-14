# oversized-file: session_hook.rs

**Location:** `packages/tddy-tools/src/session_hook.rs`
**Category:** oversized-file
**Detected:** 2026-11-03 — `/pr-wrap` step 3.5 file-length gate on #796 (`#agent-tools` 7/12), production lines counted to the first `#[cfg(test)]`
**Metrics:** **547 production lines** · budget 500 · **1.1× over**
**Thresholds breached:** length 547 > 500
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-13 (#883 forwarded the hook's conversation fields)
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-11-03 | 498 → 535 | #796 (`#agent-tools` 7/12) pushed it past the budget: `HookTransport` / `hook_transport` (the tool-IPC socket in a jail, the daemon socket on the host, HTTP otherwise), `exchange` and `call_over_socket`. Not split: the developer deferred all code-restructuring decisions in `#agent-tools` to the stack's sweep node (developer, 2026-11-02) |
| 2026-11-13 | 535 → 547 | #883 (`#orchestrators` 6/11) forwards `hook_event_name`, `agent_conversation_id` and `hook_fields` on every report. Not split: `#orchestrators` 7/11 (`stack-plan-tools`) also touches the file, and a stack-shared file is not split mid-stack |

## What would close it

Move the transport selection and its socket/HTTP exchange (`HookTransport`, `hook_transport`, `exchange`, `call_over_socket`, `post_proto`, ~110 lines) into a `session_hook/transport.rs` submodule.

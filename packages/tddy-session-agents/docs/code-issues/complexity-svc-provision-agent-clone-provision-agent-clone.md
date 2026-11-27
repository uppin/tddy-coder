# complexity: provision_agent_clone

**Location:** `packages/tddy-session-agents/src/svc_provision_agent_clone.rs` — `AgentRoster::provision_agent_clone`
**Category:** complexity
**Detected:** 2026-10-24 — measured at `/pr-wrap` of `#start-progress` 4/8 (#737), structural scan (fn line to closing brace)
**Metrics:** **86 lines** · 6 parameters
**Thresholds breached:** length 86 > 60 (`/analyze-clean-code`)
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — **unclaimed**. **Deferred with the developer's consent, #start-progress 4/8**
**Verified:** ⚠ **not hand-verified** — metrics are machine-measured and re-derivable

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-24 | 86 | first detection. 84 at `origin/feature/start-progress/supervisor-handoff` (already over the ceiling, unrecorded); #737 added the `progress` parameter and the `ForwardedStartFailure::into_status` mapping (+2) when the forward became relayed |
| 2026-11-27 | 86 | **unchanged by #1048** (`#fs-port` 4/13): `provision_agent_clone` is byte-identical on `origin/feature/fs-port/daemon-fs` and HEAD (fn line to closing brace, 86 lines both). The +8 in the file is the `records_a_session` → `records_a_session_on` port split, a different function. Still open, unclaimed (the #start-progress 4/8 deferral stands) |

## What would close it

Extract the request construction (`StartSessionRequest` for the codebase host's agent clone) and the
"peer answered with a different id" teardown into their own functions, leaving the forward and its
outcome in the body.

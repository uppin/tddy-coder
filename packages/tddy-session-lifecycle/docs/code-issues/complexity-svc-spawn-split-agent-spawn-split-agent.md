# complexity: spawn_split_agent

**Location:** `packages/tddy-session-lifecycle/src/connection_service/svc_spawn_split_agent.rs` — `spawn_split_agent`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518; **missed by the #507 sweep**
**Metrics:** **233 lines** · **nesting 5** · **9 parameters** · budget 60 / 4 / 5
**Thresholds breached:** length 233 > 60; nesting 5 > 4; parameters 9 > 5
**Restructure:** `extract_method --variant module` for the length; an options struct for the parameters
**Status:** Open — **regressed 2026-09-19**

## Measurement history

| Run | Lines | Nesting | Params | Note |
|---|---|---|---|---|
| 2026-09-19 | 233 | 5 | 9 | 213 → 233 in PR #518; **nesting crossed 4 → 5** |
| 2026-09-23 | 251 | — | 9 | touched by #520 (`#carve` 11/12) and **unchanged by it**: the room roster argument became a builder closure (`|| Arc::new(self.clone()).session_room_roster()`), same line count. 251 on master before #520 — the 233 → 251 growth predates it and is unattributed; nesting not re-derived |

## What grew it

PR #518 made `livekit` an `Option<&SplitLiveKitRoom>` so the co-located sandboxed-codebase placement
could reuse this function without a room. That added the branch selecting between
`split_remote_tool_env` and `colocated_jail_tool_env`, which is what took nesting from 4 to 5.

The trade was deliberate and is the right one — one spawn path for both placements beats two — but
it is recorded here rather than absorbed silently.

## What would close it

- **Parameters:** a `SplitAgentSpawn { os_user, session_id, sessions_base, codebase_instance_id,
  codebase_session_id, livekit, req, progress }` struct, in the shape `SandboxedCodebaseParams`
  already uses in the sibling module.
- **Length:** the attachment materialisation and the context-dir build are contiguous runs inside
  the body and are the natural `extract_method --variant module` cuts.

Note the file itself is over budget at 502 production lines — see
`oversized-file-svc-spawn-split-agent.md`, whose seam moves `delete_paired_codebase_session` and
`tear_down_codebase_session` out but leaves this function where it is.

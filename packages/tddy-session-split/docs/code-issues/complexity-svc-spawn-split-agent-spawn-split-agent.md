# complexity: spawn_split_agent

**Location:** `packages/tddy-session-split/src/svc_spawn_split_agent.rs:57` — `spawn_split_agent`
**Category:** complexity
**Detected:** 2026-09-19 — `/analyze-clean-code` on PR #518; **missed by the #507 sweep**
**Metrics:** **233 lines** · **nesting 5** · **9 parameters** · budget 60 / 4 / 5
**Thresholds breached:** length 233 > 60; nesting 5 > 4; parameters 9 > 5
**Restructure:** `extract_method --variant module` for the length; an options struct for the parameters
**Status:** Open — narrowed 2026-09-24 by #524 (254 → 110); parameters unchanged — **unclaimed**
**Moved:** 2026-10-09 by `#carve` 21/21 (#536), engine move into `tddy-session-split`, from `packages/tddy-session-lifecycle/src/connection_service/svc_spawn_split_agent.rs:57`

## Measurement history

| Run | Lines | Nesting | Params | Note |
|---|---|---|---|---|
| 2026-09-19 | 233 | 5 | 9 | 213 → 233 in PR #518; **nesting crossed 4 → 5** |
| 2026-09-23 | 251 | — | 9 | touched by #520 (`#carve` 11/12) and **unchanged by it**: the room roster argument became a builder closure (`|| Arc::new(self.clone()).session_room_roster()`), same line count. 251 on master before #520 — the 233 → 251 growth predates it and is unattributed; nesting not re-derived |
| 2026-09-23 | 254 | 5 | 9 | 251 on `origin/master` (`4e260d7f`, after #520; grown since the first row by other merges) → 254 after #508 (`#keyring` 1/9): `split_remote_tool_env` takes this daemon's `SessionTokens` and a `SplitSpawnTarget` literal instead of four loose arguments, and the room poller's minter is built from `SessionTokens` instead of `livekit.api_secret`. Nesting and signature unchanged |
| 2026-09-24 | 110 | — | 9 | #524: plan `05` (4 extract-methods, the teardown out), DRY #4 (`attached_initial_prompt`) and plan `20` (the tool wiring and the process spawn): 254 → 110 by the plan's count. The signature is untouched, so still 9 parameters; nesting not re-derived |
| 2026-10-05 | 112 | — | 9 | touched by the same-crate moves and **unchanged by them**: `write_claude_hooks_settings` and `resolve_start_session_claude_binary` are named through `service_util`. 112 at `origin/master` and at HEAD by fn line to closing brace (the 110 above is #524's plan count of the same function). Parameters unchanged |
| 2026-10-05 | 112 | — | 9 | touched by #532 (`#carve` 17/21) and **unchanged by it**: the file lost `split_forward_deadline` to `agent_roster.rs` (21 lines); the function is 112 lines at `origin/master` and at HEAD (fn line to closing brace), now at `:57`. Parameters unchanged |
| 2026-10-07 | 118 | — | 9 | touched by `#carve` 18/21 (`SplitSessions`): **+6** (112 to 118, fn line to closing brace at `4157e47f` and at HEAD). The method moved to `impl SplitSessions`; `self.attached_initial_prompt(..)` became the free `attached_initial_prompt(&self.attachment_state(), ..)`, which rustfmt wraps over nine lines instead of three (the whole growth); `RemoteCheckout::new(Arc::new(self.clone()), ..)` and the room-roster closure now go through `self.remote_worktree_snapshots()` and `self.host.session_room_roster()` at the same line count. **The signature is untouched: still 9 parameters** (Recipe B keeps them). Now at `:57` |
| 2026-10-09 | 118 | — | — | `#carve` 21/21 (#536): **moved whole** from lifecycle into `tddy-session-split` (the receiver lifecycle's wiring crate now consumes). Length by brace matching (fn line to closing brace) is identical on `origin/master` (`468b368f9`, the old path) and on HEAD, so the move changed no length, nesting or branch; the new location is the only difference. Re-measured structurally only: complexity, CRAP and coverage were **not** re-derived (no `analyze coverage` run), so those figures stay the earlier ones. Still open, unclaimed |

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
That record closed on 2026-09-24 (#524): plan `05` moved the teardown out, and the file is 445
production lines.

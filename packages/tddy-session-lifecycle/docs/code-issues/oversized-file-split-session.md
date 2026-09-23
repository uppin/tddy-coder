# oversized-file: split_session.rs

**Location:** `packages/tddy-session-lifecycle/src/split_session.rs`
**Category:** oversized-file
**Detected:** 2026-09-19 — `/pr-wrap` step 3.5 file-length gate
**Metrics:** **647 production lines** (2026-09-23; 645 at detection) · budget 500
**Thresholds breached:** length 647 > 500
**Restructure:** `extract_module --to_file` — single seam, designed, not applied
**Status:** Open — **unclaimed** · **the cheapest of the eight**

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-19 | 645 | 605 → 645 in this PR (`colocated_jail_tool_env` + its tests) |
| 2026-09-23 | 647 | master 651 → 663 in #508's first cut (`#keyring` 1/9: the agent credential minted from `SessionTokens`), back to 647 after its `/pr-wrap` refactor (`verified_caller` formats its refusal once; `split_remote_tool_env` takes the spawn target) — split deferred to a follow-up after #keyring lands because dependents #509–#513 touch it |

## What would close it — designed seam

Module-level items throughout, so fully `anchors`-addressable. **One seam clears the budget.**

**Seam:** contiguous run L345–517 — `mint_agent_session_token`, `verified_caller`,
`RoomPollTokenMinter`, `impl RoomPollTokenMinter`, `impl …SessionTokenMinter for RoomPollTokenMinter`,
`split_remote_tool_env`, `colocated_jail_tool_env`.

**Operation:** `extract_module --to_file`, `reexport: "named"` — **required**, because
`crate::split_session::{mint_agent_session_token, RoomPollTokenMinter, split_remote_tool_env,
colocated_jail_tool_env}` are reached by module path from `svc_spawn_split_agent.rs` and
`svc_start_sandboxed_codebase_session.rs`.

**Estimate:** ~173 lines out → parent **~472**.

## Constraints a later session must know

- The in-file `#[cfg(test)]` module starts at L646 and includes `withdrawal_contract_tests`, a
  deliberate regression guard on the tool-withdrawal argv. If `named` refuses on a private helper
  the tests reach, fall back to `reexport: "glob"` so `super::*` keeps resolving.
- Exact `impl` outline names are unknown. Discover them from `anchors`' own non-adjacency refusal,
  which names the item between two non-adjacent ones (``…have `X` between them``); iterating that
  yields the full run without hand-writing any name.

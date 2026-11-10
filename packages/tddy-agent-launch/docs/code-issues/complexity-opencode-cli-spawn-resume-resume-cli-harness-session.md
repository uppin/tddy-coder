# complexity: resume_cli_harness_session

**Location:** `packages/tddy-agent-launch/src/opencode_cli_spawn/resume.rs` — `resume_cli_harness_session`
**Moved:** 2026-11-07 by #829 (`#agent-tools` 21/28), from `opencode_cli_spawn.rs` to `opencode_cli_spawn/resume.rs`
**Category:** complexity
**Detected:** 2026-11-03 — `/pr-wrap` on #797 (`#agent-tools` 8/12), fn line to closing brace on `origin/feature/agent-tools/cursor-cli` (`ca7814b57`) and HEAD; no coverage run
**Metrics:** **121 lines**
**Thresholds breached:** length 74 > 60 (`/analyze-clean-code`)
**Restructure:** not designed — deferred to the #agent-tools sweep node (developer, 2026-11-02)
**Status:** Open — regressed 2026-11-06 by #823 (97 → 121) — unclaimed
**Verified:** ⚠ **not hand-verified** — the line count is machine-measured

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-11-03 | 61 → 74 | #797: the host resume renders the spec's resume argv and wires the resume adapter's `on_exit` hook |
| 2026-11-05 | 74 (unchanged) | touched by #819 (`#agent-tools` 13/19): `resume_cli_route` reads the agent from the spec; measured fn line to closing brace on `origin/feature/agent-tools/agent-registry` (`7ab752605`) and HEAD; no coverage run. Restructuring deferred to the #agent-tools sweep node (developer, 2026-11-04) |
| 2026-11-05 | 74 → 97 | **worse by 24** in #820: the resume re-wires the session (`resumed_session_wiring`), binds the resumed conversation and the node launcher and writes the slot files on an ssh target; measured on `origin/feature/agent-tools/route-support` (`3c29698cd`) and HEAD at #820's wrap. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer (2026-11-04) |
| 2026-11-06 | 97 → 121 | **worse by 24** in #823 (`#agent-tools` 17/19): claude and cursor resume here now — the conversation is adopted (`resumed_or_adopted_conversation`, recording a minted `cursor_chat_id`), the session pointer is written and its removal armed on exit, and a split session's re-derived flags and transport are applied; measured fn line to closing brace on `origin/feature/agent-tools/spec-backend` (`d325fba73`) and HEAD; no coverage run. Not split: restructuring deferred to the `#agent-tools` sweep node (n20) by the developer |
| 2026-11-07 | 115 → 115 | **unchanged, moved** in #829 (`#agent-tools` 21/28): engine `move_item` into `opencode_cli_spawn/resume.rs`; the session-pointer call takes a `PointerHost`, bound on its own line, so the length holds; fn line to closing brace (this script's count; the 121 above was measured with the doc comment) on `origin/master` (`d3fe1abea`) and HEAD |
| 2026-11-07 | 115 → 117 | touched by `#agent-tools` 25/28 (#833): **+2**, fn line to closing brace on `origin/feature/agent-tools/dead-launch-code` and HEAD. A split session's pointer is chosen by session kind and a split wiring without one is refused; that selection is `split_launch_parts`, a helper beside the function (the inline form was 125) |
| 2026-11-10 | 117 → 118 | **worse by 1** across #835–#865 (`#agent-tools` 27–32), brace-matched on `b8d9b769c` against `153b3c05d`. Not extracted: the `#agent-tools` sweep (#876) scoped Tier C to functions grown by 5 or more lines or with no record (user, 6e, 2026-11-10) |

## What would close it

Share the spec render and hook wiring with `start_cli_harness_session` (one extracted helper), which takes both under 60.

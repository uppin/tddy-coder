# complexity: schema_offering

**Location:** `packages/tddy-tool-engine/src/start_session_args/schema.rs:45` — `schema_offering`
**Category:** complexity
**Detected:** 2026-10-25 — `/pr-wrap` on #751 (`#start-parity` 2/9), function length by brace matching (fn line to closing brace)
**Metrics:** **178 lines** · budget 60 · **~3× over**; nesting 3, one closure, no loops beyond iterator chains
**Thresholds breached:** length 178 > 60
**Restructure:** not designed
**Status:** Open — added by #751; deferred by developer instruction (code issues recorded, not solved, for the `#start-parity` stack) — **unclaimed**
**Verified:** read by hand 2026-10-25: the length is 28 `offer(name, property)` calls, one per argument, each carrying its description string, plus the session-type and model `if`/`then` blocks. Low risk — straight-line data, fully exercised by `tests/start_session_schema.rs` and `tests/start_session_args_parity.rs`

## Measurement history

| Run | Lines | Note |
|---|---|---|
| 2026-10-25 | 178 | first detection. Replaces `start_project_session_schema` (~55 lines, five arguments) in `project_home_tools.rs`; the argument set grew from 5 to 28. Moved into `start_session_args/schema.rs` by the same PR's file split (engine `move_item`) |

## What would close it

Make the property table data: a `const` slice of `(name, kind, description)` for the plain string and
boolean arguments (all but `session_type`, `daemon_instance_id`, `recipe`, `model`,
`branch_worktree_intent`, `permission_mode`, `specialized_agents`, `attachments`), folded into the
`properties` map, leaves the function the enum-carrying arguments and the conditions — well under 60.
Prove the move with `restructure check --deep`; the two schema test suites pin the output.

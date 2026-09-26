# oversized-file: server.rs

**Location:** `packages/tddy-tools/src/server.rs`
**Category:** oversized-file
**Detected:** 2026-09-26 by `structural audit` — hand-measured during `/plan-red` Step 2b for the
subagent turn-control changeset
**Metrics:** **2,652 production lines** (the `#[cfg(test)]` module opens after them) · budget 500 ·
**5.3× over**
**Thresholds breached:** length 2,652 > 500
**Restructure:** required — `extract_module --to_file`, `/code-restructuring` territory
**Status:** Open — unclaimed

## Measurement history

| Run | Production lines | Note |
|---|---|---|
| 2026-09-26 | 2,488 | first detection |
| 2026-09-26 | 2,652 | +164 for `subagent_resume` (tool, schema, handler, router), `maxTurns` on both schemas, and a shared `take_a_turn`. Deferred with developer consent — see `docs/dev/todo/2026-09-26-seven-files-over-budget-deferred-by-the-subagent-turn-control-change.md` |

## What the tool found

No tool. This is a `wc -l` and a `grep -n '#\[cfg(test)\]'`, which is exactly the measurement the
`oversized-file` category is defined on — production lines before the first test module. The inline
`#[cfg(test)]` at `:39` is a branch inside `permission_relay_socket_path`, not the test module; the
module is at `:2489`.

**Not measured:** per-function complexity, nesting depth, CRAP. The CRAP pipeline
(`tddy-tools analyze coverage` + `report`) was not run for this package. "Not measured" is not
"clean" — a file this size very likely carries complexity findings too, and nobody has looked.

## Why it matters here

This one file holds **the entire `subagent_*` MCP surface** — all six tools' declarations, schema
builders, handlers and the router — alongside the permission server, the exec-tool routes and the
advertisement gating. Concretely:

- `subagent_new_session` `:1739-1799`, `subagent_prompt` `:1812-1906`, `subagent_await` `:1913-1934`,
  `subagent_cancel` `:1937-1980`, `subagent_list` `:1984-1987`, `subagent_status` `:2132+`
- the router `subagent_tool_router()` `:2353-2487` and the schema builders `:2247-2349`
- `advertised_tools()` `:400-425`, which gates the whole surface on the live roster

Any change to the subagent contract is a change to this file, and a reviewer has to hold 2,488
lines to know whether the change is local. That is the cost being paid, repeatedly: this is the
third changeset in two months to edit the subagent tool block.

## What would close it

The subagent tool surface is the obvious seam and would take roughly a thousand lines out on its
own: the six declarations, their schema builders and their handlers form a contiguous concern with
one entry point (`subagent_tool_router`) and one gate (`advertised_tools`). Anchor with
`tddy-tools restructure anchors`, never by hand, and prove it with `restructure check --deep`
against a warm index (`./run-index-daemon`).

**Deliberately not done in the changeset that detected it.** The developer chose to record rather
than restructure, so that
[`2026-09-26-subagent-turn-control-and-honest-tool-failure`](../../../../docs/dev/changesets/2026-09-26-subagent-turn-control-and-honest-tool-failure.md)
stays one reviewable PR rather than burying a behaviour change under a mechanical one. That
changeset adds a seventh tool (`subagent_resume`) and `maxTurns` to an existing one, so expect the
next measurement to be higher, not lower.

## Verified by hand

2026-09-26 — Opened the file and confirmed the `:39` `#[cfg(test)]` is an inner attribute on a block
inside a function rather than a module, so the naive "first `#[cfg(test)]`" reading (which gives 38)
is wrong and 2,488 is the real figure. Confirmed the subagent block is contiguous from `:1679`
(`SUBAGENT_PROMPT_GRACE`) through `:2487` (end of `subagent_tool_router`) with no interleaved
unrelated items, so the seam named above is real. Did **not** verify that extracting it passes
`restructure check` — that has to be run before anyone relies on the estimate.

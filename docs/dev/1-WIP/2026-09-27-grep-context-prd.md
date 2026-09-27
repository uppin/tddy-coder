# PRD — Grep context lines (`grep-context`, node 2 of `subagent-control`)

**Stack:** `subagent-control` — node 2 of 5 (wave 1). Base: `feature/subagent-control/tool-previews`.
**Branch:** `feature/subagent-control/grep-context`

## Problem

`Grep` returns matched lines only. A subagent reading a match has no way to see the code around
it — the signature above, the body below — so every match that needs context costs a second
`READ` of the whole file. The engine already shells `rg --json` (`tool_grep`,
`packages/tddy-tool-engine/src/lib.rs:436`), and ripgrep already emits `type:"context"` events
for `-B/-A/-C`; the tool just does not pass the flags and drops the events.

## What this PR delivers

`before` and `after` arguments on `Grep` (each a non-negative line count, both optional, plus
either may be given alone):

```json
{"tool": "GREP", "arguments": {"pattern": "run_turn_loop", "path": "packages/tddy-discovery",
                               "before": 2, "after": 4, "limit": 20}}
```

Result shape: each match entry gains its context lines — grouped per match as
`{path, line_number, text, context: [{line_number, text, relation: "before"|"after"}]}` — and
the caps (`truncated`/`total_matches`) keep counting **matches**, not context lines, so the
window semantics are unchanged. With no `before`/`after`, the result is byte-for-byte today's
shape.

Surfaces:

1. `tool-engine` `tool_grep` — pass `-B <n>` / `-A <n>` to rg, fold `type:"context"` events into
   the preceding/following match entries.
2. Engine catalog `ToolDef` schema + description (`catalog.rs:44`).
3. Discovery `CodebaseAccess::grep_limited` + the **Local** path (`grep_file`/`grep_dir` in
   `subagent.rs`) — both argument paths gain the fields.
4. Subagent-facing tool schema + `validate_tool_arguments` bounds (non-negative, small ceiling,
   e.g. ≤ 50 lines each side).

## Acceptance criteria

1. `Grep {pattern, before: N, after: M}` returns matches each carrying up to N `before` and M
   `after` context lines with their line numbers, via the engine path.
2. The Local path (`grep_file`/`grep_dir`) returns the same shape.
3. Without `before`/`after`, results are unchanged (no context key, no event folding).
4. `truncated`/`total_matches` still count matches; context lines never consume the window.
5. `before`/`after` beyond the ceiling are rejected by argument validation before dispatch.
6. Context lines at file edges are clamped (fewer than requested at start/end of file).

## Boundaries

- No paging/offset for Grep (the standing `glob-and-grep-cannot-be-paged` backlog entry — this PR
  does not add `offset`).
- No changes to `Glob`, `Read`, or result-summary shapes (node 1 owns `resultSummary`; a
  `contextLines` fact may ride node 1's summary only if node 1's surface already supports it —
  otherwise it is out of scope here).
- No UI changes.

## Dependencies

None — consumes nothing from node 1 beyond sharing its base branch position in the line. All
surfaces it touches exist on `master`.

## Successor PRs

None depend on this node.

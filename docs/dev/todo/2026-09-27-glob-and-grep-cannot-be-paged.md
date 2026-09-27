# 2026-09-27 — `GLOB` and `GREP` can be truncated but not paged, so their default caps lose results with no way to recover them

**Category:** Future enhancement (asymmetry left behind by removing `READ`'s default cap)
**Source:** Session `01a0e285`, and the decision that followed it to stop imposing a window on
`READ` that the model did not ask for.

## What changed, and what did not

`READ` no longer has a default window. The model passes `offset`/`limit` or gets the whole file;
`truncated` and `total_lines` come back either way. The reason is in session `01a0e285`: a
200-line cap on a 960-line file only helps a model that then **pages**, and that model did not —
it re-read the same first 200 lines **nine times** while per-turn latency climbed from 6s to 168s.

`GLOB` and `GREP` kept their default caps (`DEFAULT_GLOB_PATH_CAP = 200`,
`DEFAULT_GREP_MATCH_CAP = 100`), because they are the only bound those tools have. Session
`01a0e200`'s `GLOB **/*` returned **408,282 bytes** into a 32k-token window, and a `GREP` in the
same turn returned 180,475.

## The asymmetry

`READ` has `offset`. `GLOB` and `GREP` have only `limit`.

So a truncated `READ` is recoverable — advance `offset`, read on. A truncated `GLOB` is **not**:
the model is told `truncated: true, total_paths: 603`, is handed 200 of them, and has no argument
that would reach paths 201–603. Its only recourse is to narrow the pattern and search again, which
is a different query, not the rest of this one.

That makes the cap on those two strictly worse than `READ`'s was: it discards results, says so,
and offers no way to ask for the remainder.

## What would close it

Either of these, and they are genuinely different bets:

1. **Give them an `offset`.** `GLOB` and `GREP` gain the same window `READ` has, results stay
   capped by default, and a model that wants the rest can page for it. Keeps the bound that
   stopped the 408 KB, and removes the dead end. More work: the engine currently collects and then
   truncates, so an offset needs threading through `search_window.rs` and both call paths
   (`tool_glob`/`tool_grep` and `remote_tool_glob`/`remote_tool_grep`).
2. **Remove their defaults too**, for the same reason `READ`'s went: a cap the model did not ask
   for is one it cannot reason about. Consistent, and simpler — but it restores the 408 KB bomb
   for any model that issues a broad pattern, and unlike `READ` there is no paging story to soften
   it. Not recommended without (1) or a repeat-detector first.

**Also worth settling here:** `GREP`'s `path` argument is advertised and ignored
(`tool_grep`/`remote_tool_grep` hardcode `rg … .`), so "narrow and search again" — the only
recourse a truncated `GREP` leaves — does not currently work either. Tests for that fix are
written and staged but not landed; see the PR discussion. Fixing the cap story without fixing
`path` leaves the model with no way out of a truncated search at all.

## Why it was deferred

Removing `READ`'s cap was a one-line behaviour change with three tests to re-point. Adding a
window to two more tools crosses `tddy-discovery`, `tddy-tool-engine` and both the host and jail
execution paths, and it needs the `path` question answered first — otherwise the two halves of
"narrow your search" are fixed in the wrong order.

# 2026-09-28 — StrReplace reports how many occurrences its old_string matched

**Type:** Feature

`#subagent-control` 1/5, [#553](https://github.com/uppin/tddy-coder/pull/553). Cross-package entry:
`docs/dev/changesets/2026-09-28-subagent-tool-result-summaries.md`.

`tool_str_replace`'s result JSON carries **`matchedOccurrences`** — the count the tool already
computed internally to enforce uniqueness and discarded. It rides the success path (`1`) and the
no-match error path (`0`, still an error, payload `{"error": …, "matchedOccurrences": 0}` built by
the shared `ToolOutcome::err_json` constructor); the non-unique error is unchanged. The subagent
result summary derives `matchedLines` from it, so a caller deciding whether an edit landed reads a
number instead of inferring it from `replaced` alone.

Code issue at wrap: `docs/code-issues/oversized-file-lib.md` — 777 production lines (was 763),
+14. Open, unclaimed; restructuring deferred past the `subagent-control` stack.

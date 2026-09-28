# 2026-09-28 — Grep runs ripgrep with context flags and folds the events it gets back

**Type:** Feature

`#subagent-control` 2/5, [#554](https://github.com/uppin/tddy-coder/pull/554). Cross-package entry:
`docs/dev/changesets/2026-09-28-grep-context-lines.md`.

`tool_grep` accepts optional **`before`/`after`** (each clamped to the context ceiling of 50 — the
engine's callers are the host; discovery rejects a model-issued count past it), passes `-B`/`-A`
to `rg --json`, and folds the `type:"context"` events into their adjacent match entries via
`fold_context_events`: a shared context line attaches to exactly one match — the next match's
`before`, else the last match's `after` — and every `begin` resets the window, so nothing folds
across files. Each context entry is `{lineNumber, text, relation}` (camelCase, matching the
discovery shape both paths answer with); ripgrep's trailing newline is stripped. The catalog
`ToolDef` schema and description carry the bound (`minimum: 0, maximum: 50`) and the answer shape.
Without the arguments the result is byte for byte the context-free shape, and
`truncated`/`total_matches` keep counting matches — context lines never consume the window.

Code issue at wrap: `docs/code-issues/oversized-file-lib.md` — 869 production lines (was 789),
+80, the in-place edit the changeset planned. Open, unclaimed; restructuring deferred past the
`subagent-control` stack.

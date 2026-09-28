# 2026-09-28 — A subagent's Grep can ask for the lines around each match

**Type:** Feature

`#subagent-control` 2/5, [#554](https://github.com/uppin/tddy-coder/pull/554). Cross-package entry:
`docs/dev/changesets/2026-09-28-grep-context-lines.md`.

`Grep` gains optional **`before`/`after`** (0–50 each, either alone) end to end on both codebase
paths. The shared vocabulary and the Local path live in the new `subagent/grep_context.rs`:
`GrepContext::from_args` reads the pair — absent or non-integer reads as absent, a count past
`GREP_CONTEXT_LINE_CEILING` **rejects to `None`** rather than clamping — and
`with_local_windows` computes each match's context from the file's own lines, clamped at file
edges. `CodebaseAccess::grep_with_context` serves it (Managed forwards the args to the engine;
Local computes in-process), and `dispatch_tool_call`'s GREP arm threads the pair, calling
`grep_limited` unchanged when no context was asked. `validate_tool_arguments` faults a
past-ceiling or negative count as `ArgumentProblem::OutOfRange { allowed: "0 to 50" }`, naming the
argument, and the model-facing schema carries the bound formally. Match entries grow a bounded
`context` list (`{lineNumber, text, relation}`, file order, shared lines attached once); without
the arguments results are byte for byte today's shape, and the window keeps counting matches.

Code issue at wrap: `docs/code-issues/oversized-file-subagent.md` — 1,912 production lines (was
1,850), +62 around the planned seam; the computation itself is the new sibling. Open, unclaimed;
restructuring deferred past the `subagent-control` stack.

# 2026-09-28 — Subagent Grep answers with the lines around each match

PRD: the `grep-context` node of the `subagent-control` stack
([#554](https://github.com/uppin/tddy-coder/pull/554)).

A subagent's `Grep` accepts **`before`** and **`after`** — up to 50 lines each, either alone — and
every match entry comes back with its `context`: the lines around the match, numbered, each
spelled `before` or `after`, in file order. The signature-above, body-below question that used to
cost a `READ` of the whole file now costs the one call. Windows clamp at file edges (fewer lines
than asked rather than padding or an error), a line shared between two matches' windows is
attached exactly once, and without the arguments the result is byte for byte the context-free
shape. The window semantics do not move: `truncated` and `total_matches` keep counting matches —
context lines never consume it. Both codebase paths answer identically: the managed path folds
ripgrep's own context events; the local path computes the same windows from the file's lines.

The bound is a rejection, not a clamp: a count past 50 is faulted by argument validation naming
the argument and the allowed range, so the model re-asks within the bound instead of learning the
edge by trial and error.

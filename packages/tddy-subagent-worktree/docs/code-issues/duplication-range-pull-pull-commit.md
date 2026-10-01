# duplication: pull_commit

**Location:** `packages/tddy-subagent-worktree/src/range_pull.rs:112` — `ConversationWorktree::pull_commit`, repeating `worktree.rs:334` — `pull_into_caller`
**Category:** duplication
**Detected:** 2026-10-01 — `/analyze-clean-code` on #563 (`#agent-worktree` 4/4), read by hand
**Metrics:** **~14 repeated lines** — the `diff --name-status` / `--numstat` / `--binary` / `--name-only -z` quartet, `count_change`, then `apply_3way`
**Status:** Open — **unclaimed**
**Verified:** ✅ hand-read on 2026-10-01

## Measurement history

| Run | Repeated lines | Note |
|---|---|---|
| 2026-10-01 | ~14 | first detection; `pull_range` itself is 48 lines, under the 60-line ceiling |

## What the tool found

`pull_commit` (one commit, `c^..c`) and `pull_into_caller` (the squashed `base..branch`) run the same
sequence against a revision range: read name-status, return early when empty, read numstat, count the
change, read the binary patch, read the touched paths, apply 3-way. They differ only in the range
string and in what they do with the result (add to an outcome vs return one).

## Why it matters here

The 3-way application is the part that must not drift: a fix to how touched paths are listed or how
an empty change is detected has to land in both. `pull_into_caller` belongs to `isolated-edits`
(#560), and #563's boundary forbids changing it, so the copy was the in-scope choice.

## What would close it

Extract one `async fn apply_range(&self, range: &str) -> Result<Option<(FileChanges, LineChanges, Vec<String>)>, WorktreeError>`
in `worktree.rs`, called by `pull_into_caller` and `pull_commit`. Behaviour-preserving; the
`range_pull_acceptance` and `pull_*` suites in this crate are the baseline. Do it after the
`#agent-worktree` stack lands, since it edits #560's function.

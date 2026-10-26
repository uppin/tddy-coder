# 2026-10-25 — A cancelled start deletes the branch it cut

**Type:** Feature

`#start-parity` 4/9 ([#753](https://github.com/uppin/tddy-coder/pull/753)). `start_cut_branch.rs`:
the cancel undo reads the branch from the session's changeset (only for `new_branch_from_base` with a
daemon-managed worktree), removes the session, then `git branch -D`s it; an absent branch is fine, a
refused delete ends the stream `INTERNAL` naming it.

Tests: `tddy-session-lifecycle/tests/start_cancel_acceptance.rs`.

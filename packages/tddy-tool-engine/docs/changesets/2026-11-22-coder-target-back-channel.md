# 2026-11-22 — ReadSessionActivity pages by log position; a start keeps its branch

**Type:** Fix · PR #997 · Cross-package: [docs/dev/changesets/2026-11-22-coder-target-back-channel.md](../../../../docs/dev/changesets/2026-11-22-coder-target-back-channel.md)

- `session_follow::page_activity` pages `SequencedActivity` by `log_seq`; `SessionActivity.records` is sequenced.
- `start_ledger::apply_ending`: an empty listed branch or worktree is none, so the one the start reported stands.

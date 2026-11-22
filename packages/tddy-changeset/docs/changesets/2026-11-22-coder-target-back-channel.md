# 2026-11-22 — Activity read with its log position and its start time

**Type:** Fix · PR #997 · Cross-package: [docs/dev/changesets/2026-11-22-coder-target-back-channel.md](../../../../docs/dev/changesets/2026-11-22-coder-target-back-channel.md)

- `read_all_sequenced_agent_activity` / `SequencedActivity { log_seq, record }`: `log_seq` is the 1-based line of a call's latest row.
- Coalescing is field-wise: a `PostToolUse` row keeps the `PreToolUse` row's start time and input.

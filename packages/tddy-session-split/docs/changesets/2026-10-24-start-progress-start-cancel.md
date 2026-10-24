# 2026-10-24 — Cancelling a split start cancels the codebase host's half

**Type:** Feature

`#start-progress` 8/8 (#741). `start_split_claude_cli_session` takes the start's `CancellationToken`: `forward_split_start` remembers the codebase host's `started` (`CodebaseStartTask`), and on cancel `cancel_codebase_half` forwards `CancelTask` for it and tears the workspace session down. 153 → 170 lines (code issue kept, deferred with the developer's consent). Cross-package entry: [changeset](../../../../docs/dev/changesets/2026-10-24-start-progress-start-cancel.md).

# 2026-11-22 — A question held by its PermissionRequest hook is answered

**Type:** Fix · PR #997 · Cross-package: [docs/dev/changesets/2026-11-22-coder-target-back-channel.md](../../../../docs/dev/changesets/2026-11-22-coder-target-back-channel.md)

- `session-hook --await-answer`: `PermissionRequest` of `AskUserQuestion` → `await_question_decision(…, event)` → `question_decision_json_for`.

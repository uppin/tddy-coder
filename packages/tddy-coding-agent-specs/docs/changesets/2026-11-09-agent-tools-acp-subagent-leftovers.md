# 2026-11-09 — A headless spec declares who answers its permission requests

**Type:** Feature

`#agent-tools` 27/32 ([#835](https://github.com/uppin/tddy-coder/pull/835)).

- `HeadlessSection.permission_policy: Option<HeadlessPermissionPolicy>` — `asks_caller` or
  `{ fixed: <text> }`; declared in `claude`, `cursor`, `codex` and `oz.yaml`. Test:
  `tests/acp_slots_unit.rs` (`every_built_in_headless_spec_declares_who_answers_its_permission_requests`).

See [coding-agent-specs.md](../coding-agent-specs.md) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-agent-tools-acp-subagent-leftovers.md).

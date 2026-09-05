# 2026-07-03 — Managed-codebase workflow controls

**Type:** Feature

`CreateSessionPane` replaces the implied managed flag with an explicit "Managed codebase" checkbox that, when enabled for a claude-cli session, reveals a workflow-recipe picker and the specialized-subagents multi-select and sends explicit `managed_codebase` + `recipe` (subagents gated on the toggle so a hidden selection can't leak). Feature [managed-codebase-workflow.md](../../../../docs/ft/coder/managed-codebase-workflow.md). Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-web)

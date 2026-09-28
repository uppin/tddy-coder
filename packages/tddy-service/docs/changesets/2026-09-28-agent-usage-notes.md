# 2026-09-28 — models.proto carries the assistant's usage notes

**Type:** Feature

`#subagent-control` 3/5, [#555](https://github.com/uppin/tddy-coder/pull/555). Cross-package entry:
`docs/dev/changesets/2026-09-28-agent-usage-notes.md`.

`models.proto`: optional `usage_notes` on `AssistantEntry`, `CreateAssistantRequest` and
`UpdateAssistantRequest`. Updates carry the whole replacement value, like `replaces`.

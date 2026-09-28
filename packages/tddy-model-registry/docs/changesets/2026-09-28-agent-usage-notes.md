# 2026-09-28 — Assistants carry their operator's usage notes

**Type:** Feature

`#subagent-control` 3/5, [#555](https://github.com/uppin/tddy-coder/pull/555). Cross-package entry:
`docs/dev/changesets/2026-09-28-agent-usage-notes.md`.

The `assistant` table gains an optional **`usage_notes`** column (+ migration), carried on
`AssistantEntry`, `CreateAssistantRequest` and `UpdateAssistantRequest` (proto `models.proto`;
updates replace the whole value, like `replaces`), round-tripped by the store's
create/update/list, and projected by `assistant_to_agent_def` into
`SpecializedAgentDef.usage_notes`. It is **operator documentation, never machine context** — no
prompt, spawn or session surface reads it.

Code issue at wrap: `docs/code-issues/oversized-file-store.md` — 1,152 production lines (was
1,139), +13. Open, unclaimed.

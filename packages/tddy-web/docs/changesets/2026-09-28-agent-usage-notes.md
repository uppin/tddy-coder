# 2026-09-28 — Usage notes on the Models screen's assistants

**Type:** Feature

`#subagent-control` 3/5, [#555](https://github.com/uppin/tddy-coder/pull/555). Cross-package entry:
`docs/dev/changesets/2026-09-28-agent-usage-notes.md`.

The create and edit assistant dialogs gain a multi-line **usage notes** field (testids
`models-create-assistant-usage-notes` / `models-edit-assistant-usage-notes`; edit pre-fills the
stored text), the fan-out hook carries `usageNotes` on both RPC payloads whole — the same
carried-whole rule `replaces` follows — and the panel row shows the note truncated with the full
text on hover, only when non-empty. Operator documentation, never machine context.

Code issue at wrap: `docs/code-issues/oversized-file-use-model-registry-fan-out.md` — the
package's first record (615 lines, +17 this PR); decomposition deferred by explicit developer
consent, see `docs/dev/todo/2026-09-28-web-model-fan-out-over-budget.md`.

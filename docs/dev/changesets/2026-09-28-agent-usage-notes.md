# 2026-09-28 — An agent carries usage notes for its operator

**Type:** Feature

`#subagent-control` 3/5, [#555](https://github.com/uppin/tddy-coder/pull/555) (base:
`feature/subagent-control/grep-context`, node 2). Order-only on both parents — nothing consumed.
Nothing downstream depends on this node.

Packages: `tddy-model-registry` (column + migration, store CRUD, projection), `tddy-discovery`
(the `SpecializedAgentDef` field), `tddy-service` (`models.proto`), `tddy-web` (dialogs, panel,
RPC fan-out).

## What was delivered

An assistant in the model registry carries an optional **`usage_notes`** — its operator's
documentation of how to use it: quirks, budgets, what not to ask it. The field is optional
everywhere, following the `api_key` serde precedent and the update's carried-whole rule.

- **Registry**: a column on `assistant` (+ migration), fields on `AssistantEntry`,
  `CreateAssistantRequest` and `UpdateAssistantRequest`, store create/update/list round-trip, and
  the `assistant_to_agent_def` projection into `SpecializedAgentDef.usage_notes`.
- **Discovery**: the def field, serde + `Debug` redaction, and the agent-def YAML example
  (`docs/ft/coder/specialized-subagents.md`).
- **Web**: a multi-line field on the create and edit dialogs (pre-filled on edit), the notes
  carried **whole** on both RPC payloads (the same rule `replaces` follows), and the assistant's
  panel row showing the note truncated with the full text on hover — so an operator choosing an
  agent reads how to use it before prompting it.

**The boundary that shapes the feature:** usage notes are **operator documentation, never machine
context**. No prompt, spawn, turn or session surface reads them — not injected into system
prompts, not on `subagent_new_session`, not on `SessionAgentEntry`/`SubagentInfo` pickers.

## Tests

Three store/projection acceptance tests (green in the draft contract), and three Cypress
component tests (create submit, edit pre-fill + update, panel display). The create spec's first
test had to be reordered to type-then-submit: it typed its notes field after a submit click that
closes the dialog on success — no implementation could satisfy it; intent and assertions
unchanged.

## Code issues and file length at wrap

- `packages/tddy-model-registry/docs/code-issues/oversized-file-store.md` — 1,152 production
  lines (was 1,139 at the merge-base), +13: the column + migration and the `update_assistant`
  signature widening. Open, unclaimed.
- `packages/tddy-web` had **no `docs/code-issues/`** at all — the package was unanalyzed, not
  clean. First record filed:
  `oversized-file-use-model-registry-fan-out.md` (598 → 615, +17). Decomposition **deferred by
  explicit developer consent** at wrap, recorded in
  `docs/dev/todo/2026-09-28-web-model-fan-out-over-budget.md`.

## Backlog

No prior `docs/dev/todo/` entry touched. One added by the deferral above; nothing resolved.

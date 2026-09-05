# 2026-07-03 — Managed codebase workflow (workflow-aware Claude CLI)

- New-session "Managed codebase" is now an explicit checkbox for claude-cli sessions that, when enabled, reveals both a workflow-recipe picker and the specialized-subagents multi-select (previously recipe selection was tool-only and "managed" was implied by picking ≥1 subagent)
- A managed claude-cli session (sandboxed or not) is launched *workflow-aware*: the daemon injects the recipe's orchestration system prompt (`--append-system-prompt-file`) and wires the `transition` tool to a per-session `WorkflowController`, so Claude advances and persists its own workflow state in `changeset.yaml`
- `transition` is relayed on the host over the existing `TDDY_SOCKET` protocol (no `tddy-tools`/proto changes); the transition handler became per-instance so concurrent managed sessions never cross-route
- Resume of a managed session re-wires the workflow and resumes at the persisted goal (not the start goal)
- An unknown recipe on a managed claude-cli session is rejected with `INVALID_ARGUMENT`
- Feature: [managed-codebase-workflow.md](../managed-codebase-workflow.md)

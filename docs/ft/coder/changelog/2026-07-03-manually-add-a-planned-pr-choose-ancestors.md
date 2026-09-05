# 2026-07-03 — Manually add a planned PR + choose ancestors

- New "+ New planned PR" form on the PR-Stack Chat Screen lets the operator manually add a planned PR node and pick its ancestors via a multi-select checkbox picker over the orchestrator's existing planned-PR nodes — no chat/LLM round trip required
- New `AddPlannedPr` RPC appends one `StackNode` to a `"pr-stack"` orchestrator's `Changeset.stack`, additive-only (never rewrites existing nodes, unlike the chat-driven plan refinement), server-assigns the node id, and validates ancestors/cycles before writing
- Feature: [pr-stacking.md § Manually adding a planned PR](../pr-stacking.md#manually-adding-a-planned-pr)

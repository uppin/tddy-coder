# 2026-11-11 — OpenCode: does `tokens_output` already include `tokens_reasoning`?

**Category:** Deferred from `#usage-binding 5/5` (#894)
**Source:** `docs/dev/changesets/2026-11-11-usage-binding-opencode-sqlite.md`; `packages/tddy-coding-agent-specs/specs/opencode.yaml`

The only local `opencode.db` holds no sessions, so the overlap could not be checked. `opencode.yaml`
maps `tokens_output` → `output` and `tokens_reasoning` → `reasoning` as disjoint columns. If a reasoning
model's `tokens_output` already includes its reasoning tokens, the Usage tab double-counts them.
`turns` also reads 0 for OpenCode (`session_v2` has no per-turn column; counting `session_message` rows
would need a second query shape).

**What would close it:** run a reasoning model in OpenCode and read its `session_v2` row. If
`tokens_output >= tokens_reasoning` includes it, change the binding to
`output: "tokens_output - tokens_reasoning"` (the `CounterExpr` grammar already applies to a `sqlite`
source) and add a case to `opencode_binding_acceptance.rs`.

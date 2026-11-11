# 2026-11-11 — ACP `usage_update` cost is not bound for any agent

**Category:** Deferred from `#usage-binding 3/5` (#892), re-deferred by `#usage-binding 4/5` (#893)
**Source:** `docs/dev/changesets/2026-11-10-usage-binding-codex-rollout.md`; `packages/tddy-coding-agent-specs/specs/claude-acp.yaml`, `codex-acp.yaml`

`aggregate: last` now exists, which is what ACP's cumulative `usage_update.cost` needed. Neither
`claude-code-acp` nor `codex-acp` has a recorded `usage_update` in the repo, so a cost binding would be
invented field names, and none is declared. The ACP tap (`acp.usage_events`) ships and is tested.

**What would close it:** record a real ACP stream from each adapter and, for each, add
`acp.usage_events` for the `usage_update` session update plus a second, cost-only source reading
`{session_dir}/usage/{conversation_id}.jsonl` with `aggregate: last`, `cost: <amount path>` and a
currency predicate, with a test in `tddy-coding-agent-specs/tests/usage_section_acceptance.rs` over the
recorded line.

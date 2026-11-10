# complexity: lower_tool_jail

**Location:** `packages/tddy-daemon-sandbox/src/tool_jail.rs:76` — `lower_tool_jail`
**Category:** complexity
**Detected:** 2026-11-02 — `/pr-wrap` step 4 (`/analyze-clean-code`) on PR #794 (`#agent-tools` 5/12)
**Metrics:** **68 lines** · budget 60
**Thresholds breached:** length 68 > 60
**Restructure:** `extract_method` — `/code-restructuring` territory
**Status:** Open — deferred to the #agent-tools sweep node (developer, 2026-11-02) — **unclaimed**

## Measurement history

| Run | Metric | Note |
|---|---|---|
| 2026-11-02 | 68 lines | first detection; new in this PR |

## What the tool found

Lowers home, files, secrets, policy, network and tools for one jail and merges their reads, links and env into `ToolJailGrants`. Linear, no nesting; the length is the merge.

## What would close it

Extract the merge of `tool_grants` and `file_grants` (reads, symlinks, env with `HOME`) into a helper.

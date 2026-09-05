# 2026-06-06 — Stable chain parent session id in `tcp:` callback

**Type:** Fix

format changed from `tcp:<idx>|s:<child>` to `tcp:p:<parent_tail8>|s:<child>` (last 8 chars of parent id); `handle_chain_parent_callback` scans candidates by tail instead of index — immune to list churn. `parse_chain_workflow_prompt` unit tests added. (tddy-daemon)

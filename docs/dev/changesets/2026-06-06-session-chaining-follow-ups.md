# 2026-06-06 — Session chaining follow-ups

**Type:** Fix

`tcp:` chain callback format changed to embed `parent_tail8` (last 8 chars of parent session id) instead of list index — stable across session churn; `parse_chain_workflow_prompt` unit tests. (tddy-daemon)

# 2026-10-24 — Shared pieces for the coding-agent session
**Type:** Feature

`SubagentError::unsupported_by_coding_agent(option)`; `preview_of` and `MessageId::from_ordinal` are public for the coding-agent session's message descriptors; seeded roster entries are stamped `kind: MODEL` (#721, `#coagents` 2/8). Code issues: `oversized-file-subagent` 2,046 → 2,058 and new `oversized-file-registry` (805), both stack-shared, split deferred.

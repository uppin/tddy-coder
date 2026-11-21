# 2026-11-21 — The startup liveness probe of a target agent has a round trip to answer

**Type:** Bug Fix · **Stack:** `#target-parity` 5/7 (PR #973)

`exits_during_startup`: a probe is bounded by `start_timeout`, only an answered probe confirms the agent alive, and probing is capped at `startup_window + start_timeout` (`RemoteLaunchError::AgentNeverAnswered`). Full record: [`docs/dev/changesets/2026-11-21-target-startup-liveness.md`](../../../../docs/dev/changesets/2026-11-21-target-startup-liveness.md).

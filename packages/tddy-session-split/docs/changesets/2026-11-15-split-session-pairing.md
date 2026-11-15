# 2026-11-15 — The agent half's first changeset records the codebase branch

**Type:** Fix

`split_codebase_branch.rs`: `codebase_branch_of` reads the branch the codebase
host lists for its codebase half (a local changeset read when this daemon holds
that half), and `record_first_changeset` writes the agent half's first
changeset — that branch, plus the orchestrator when one started it — before
the agent is spawned, in place of `record_orchestrator_before_spawn`. A branch
that cannot be read fails the start; the caller's `Err` path tears the
codebase half down.

Cross-package record: [docs/dev/changesets/2026-11-15-split-session-pairing.md](../../../../docs/dev/changesets/2026-11-15-split-session-pairing.md).

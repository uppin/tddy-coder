# 2026-11-04 — `spec_for_session_type` is replaced by the registry but still has one caller

**Category:** Dead code
**Source:** `#agent-tools` 12/12 (#801), `TODO(agent-registry)` at `packages/tddy-coding-agent-specs/src/lib.rs`

## What is there

`CodingAgentRegistry::for_session_type` resolves a CLI session type to its agent from the host's
registry, so a dropped-in `<id>.yaml` has one. The closed table `spec_for_session_type` (five session
types) is read only by `harness_profile_start::profile_spec_id` and its own unit tests. It is kept, marked,
because deleting it is not on the changeset's *Removed* list.

## What to decide

Resolve `profile_spec_id` through the registry, then delete the function, its two unit tests and the doc references.

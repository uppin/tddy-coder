# 2026-08-30 — cross-host planned-PR visibility

**Type:** Bug Fix + Feature

the `session` participant block's shape moves to `tddy_core::session_participant_metadata` (two crates publish it now, and the merge is shallow), and the seed gains the session's stack association from a new `--stack-node-id` flag plus `--stack-parent` and the changeset's branch, so the first publish already carries it; a changeset read error on that path is logged rather than silently yielding an empty branch.

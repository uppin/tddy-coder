# 2026-08-30 — cross-host planned-PR visibility

**Type:** Docs

`participant-metadata.md` records the `session` block's second publisher (`tddy-daemon`, for claude-cli sessions, re-sending on a 30s timer) and its three new stack-association fields; no code change in this crate — `spawn_local_participant_metadata_watcher` was already public and already shallow-merges.

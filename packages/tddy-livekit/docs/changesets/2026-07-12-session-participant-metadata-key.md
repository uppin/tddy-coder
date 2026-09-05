# 2026-07-12 — `session` participant metadata key

**Type:** Docs

documents the `session` metadata block published by `tddy-coder`'s participant (sibling of `owned_project_count` / `codex_oauth`, shallow-merged via `merge_participant_metadata_json`): `session_id`, `workflow_goal`, `workflow_state`, `elapsed_display`, `agent`, `model`, `activity_status`, `recipe`, `repo_path`, `pending_elicitation`. No new public API; the merge helper already existed. `participant_metadata_unit` extended to assert the `session` key is preserved across merges. Feature [participant-metadata.md § `session` metadata key](../participant-metadata.md#session-metadata-key). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#297](https://github.com/uppin/tddy-coder/pull/297). (tddy-livekit)

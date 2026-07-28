# Changesets Applied

Wrapped changeset history for tddy-workflow.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-27** [Feature] session-attachment-store: `artifact_paths` gains `SESSION_ATTACHMENTS_SUBDIR`, `session_attachments_root`, and `canonical_attachment_write_path` (`session_dir/artifacts/attachments/<basename>`). Docs [architecture.md](architecture.md). Feature [session-attachments.md](../../../docs/ft/coder/session-attachments.md). Cross-package [docs/dev/changesets.md](../../../docs/dev/changesets.md). (tddy-workflow)
- **2026-03-28** [Architecture] Recipe-owned planning artifacts — New crate: **`artifact_paths`** (`session_artifacts_root`, `resolve_existing_session_artifact`, `resolve_existing_primary_planning_document`, `read_session_artifact_utf8`) so **`tddy-core`** does not default primary PRD basenames; **`tddy-workflow-recipes`** owns manifest-driven paths. (tddy-workflow, tddy-core, tddy-workflow-recipes, tddy-coder, tddy-service, tddy-integration-tests)

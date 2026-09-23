# Architecture

## Purpose

**`tddy-workflow`** holds workflow-neutral helpers for **where** session artifacts live on disk, so the workflow engine does not embed fixed basenames (such as `PRD.md`) or ad-hoc path rules.

Recipes in **`tddy-workflow-recipes`** combine **`SessionArtifactManifest`** (defaults and known keys) with these functions to resolve reads and elicitation.

## `hints` module

- **`PermissionHint`**: `ReadOnly` or `AcceptEdits`. Backend-agnostic; each backend maps it to its
  own flags (Claude permission mode, Cursor and Codex sandbox options).
- **`GoalHints`**: per-goal backend configuration a recipe produces — display name, permission,
  allowed tools, default model, whether agent output streams, vendor plan mode, and whether a Claude
  non-zero exit counts as success when stdout carries a structured response.

Both are plain data that a backend reads and a recipe produces. Keeping them here keeps
`tddy-agent-backend` below `tddy-workflow-engine`. `tddy_core::workflow::recipe::{GoalHints,
PermissionHint}` still resolve, through the engine's re-export.

## `artifact_paths` module

- **`session_artifacts_root`**: `session_dir/artifacts/` for new layouts.
- **`canonical_artifact_write_path`**: preferred write path under `artifacts/` for a basename.
- **`SESSION_ATTACHMENTS_SUBDIR`** / **`session_attachments_root`**: `session_dir/artifacts/attachments/` for user-attached documents (recipe-independent session artifacts).
- **`canonical_attachment_write_path`**: preferred write path under `artifacts/attachments/` for a basename (no validation — policy lives in **`tddy-daemon`** `session_attachments`).
- **`resolve_existing_session_artifact`**: resolves an existing file preferring `artifacts/`, then a legacy layout under `sessions/<uuid>/`, then the session directory root.
- **`resolve_existing_primary_planning_document`**: resolves the recipe’s primary planning document (e.g. `prd` key) using the same search order.
- **`read_session_artifact_utf8`**: reads UTF-8 from a resolved path when present.

Callers include TDD hooks (`before_*` / `after_*`), `TddRecipe::read_primary_session_document_utf8`, the daemon attachment store, and integration tests that assert layout behavior.

Product contract for attachments: [session-attachments.md](../../../docs/ft/coder/session-attachments.md).

## Related

- **`tddy-workflow-engine`** (re-exported by `tddy-core`): `WorkflowRecipe` trait (`uses_primary_session_document`, `read_primary_session_document_utf8`).
- **`tddy-workflow-recipes`**: `SessionArtifactManifest`, `TddRecipe`, hook implementations.

# Architecture

## Purpose

**`tddy-workflow`** holds workflow-neutral helpers for **where** session artifacts live on disk, so **`tddy-core`** does not embed fixed basenames (such as `PRD.md`) or ad-hoc path rules.

Recipes in **`tddy-workflow-recipes`** combine **`SessionArtifactManifest`** (defaults and known keys) with these functions to resolve reads and elicitation.

## `artifact_paths` module

- **`session_artifacts_root`**: `session_dir/artifacts/` for new layouts.
- **`canonical_artifact_write_path`**: preferred write path under `artifacts/` for a basename.
- **`resolve_existing_session_artifact`**: resolves an existing file preferring `artifacts/`, then a legacy layout under `sessions/<uuid>/`, then the session directory root.
- **`resolve_existing_primary_planning_document`**: resolves the recipe’s primary planning document (e.g. `prd` key) using the same search order.
- **`read_session_artifact_utf8`**: reads UTF-8 from a resolved path when present.

Callers include TDD hooks (`before_*` / `after_*`), `TddRecipe::read_primary_session_document_utf8`, and integration tests that assert layout behavior.

## Related

- **`tddy-core`**: `WorkflowRecipe` trait (`uses_primary_session_document`, `read_primary_session_document_utf8`).
- **`tddy-workflow-recipes`**: `SessionArtifactManifest`, `TddRecipe`, hook implementations.

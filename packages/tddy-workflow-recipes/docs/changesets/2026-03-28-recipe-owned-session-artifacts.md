# 2026-03-28 — Recipe-owned session artifacts

**Type:** Architecture

**`SessionArtifactManifest`**; TDD hooks and **`TddRecipe::read_primary_session_document_utf8`** use **`tddy_workflow::artifact_paths`** for `artifacts/` vs legacy layout; **`before_update_docs`** builds availability from **`known_artifacts()`**; no core fallback for primary PRD basename. (tddy-workflow-recipes, tddy-workflow, tddy-core)

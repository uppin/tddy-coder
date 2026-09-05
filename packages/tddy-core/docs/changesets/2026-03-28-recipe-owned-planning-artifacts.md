# 2026-03-28 — Recipe-owned planning artifacts

**Type:** Architecture

Removed **`session_plan_prd`**; **`WorkflowRecipe`** gains **`uses_primary_session_document`** / **`read_primary_session_document_utf8`**; presenter, CLI, and service call sites use recipe I/O; no hard-coded **`PRD.md`** default in core. Guard test in **`lib.rs`** prevents re-export of legacy PRD path helpers. See **`tddy-workflow`** **`artifact_paths`**. (tddy-core, tddy-workflow, tddy-workflow-recipes)

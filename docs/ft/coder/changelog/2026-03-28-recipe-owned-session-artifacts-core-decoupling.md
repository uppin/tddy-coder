# 2026-03-28 — Recipe-owned session artifacts (core decoupling)

- **Behavior**: Primary planning document paths and **`session_dir/artifacts/`** layout are driven by **`WorkflowRecipe`** + **`SessionArtifactManifest`** and **`tddy-workflow`** path helpers, not hard-coded **`PRD.md`** defaults in **`tddy-core`**.
- **API**: **`WorkflowRecipe::uses_primary_session_document`** / **`read_primary_session_document_utf8`** for approval, CLI, and daemon; TDD recipe behavior unchanged (**`prd` → `PRD.md`** in manifest).
- **Docs**: [workflow-recipes.md](../workflow-recipes.md) (session artifacts section), package architecture notes under **`tddy-core`**, **`tddy-workflow`**, **`tddy-workflow-recipes`**.

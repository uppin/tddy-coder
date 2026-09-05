# 2026-04-04 — Projects: `main_branch_ref` (git integration base)

- **Registry**: Optional **`main_branch_ref`** on project rows; **`effective_integration_base_ref_for_project`**; **`add_project`** rejects invalid refs before **`projects.yaml`** writes (**`tddy_core::validate_integration_base_ref`**).
- **Docs**: [git-integration-base-ref.md](../../coder/git-integration-base-ref.md), [project-concept.md](../project-concept.md); package [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md).
- **PRD retired**: Prior WIP PRD for the multi-user daemon was merged into [project-concept.md](../project-concept.md) (**Multi-user daemon**) and this changelog; source file removed from **`docs/ft/daemon/1-WIP/`**.

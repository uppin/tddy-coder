# 2026-04-05 — Chain PR optional integration base (worktrees)

- **tddy-core**: **`validate_chain_pr_integration_base_ref`**, **`fetch_chain_pr_integration_base`**, **`setup_worktree_for_session_with_optional_chain_base`**, **`resolve_persisted_worktree_integration_base_for_session`**; **`Changeset`** fields **`effective_worktree_integration_base_ref`**, **`worktree_integration_base_ref`** on **`changeset.yaml`**.
- **tddy-integration-tests**: **`chain_pr_base_acceptance`** (default base, selected **`origin/...`** base, persistence, validation, resume resolution).
- **Docs**: [git-integration-base-ref.md](../git-integration-base-ref.md); **`packages/tddy-core/docs/architecture.md`**, **`packages/tddy-core/docs/changesets.md`**; cross-package **[docs/dev/changesets/](../../../dev/changesets/)**.

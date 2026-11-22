# 2026-11-22 — The pr-stack recipe's removal (`#workqueue` 1/14, #1002)

**Type:** Feature (removal)

The recipe dropdown offers no stack recipe; the stack parent and stack base pickers, `useCreateSessionStack` and `stackParents.ts` are deleted. A child's `stackParent` / `stackNodeId` come only from `initialValues`. Cross-package: [2026-11-22-remove-pr-stack-recipe.md](../../../../docs/dev/changesets/2026-11-22-remove-pr-stack-recipe.md).

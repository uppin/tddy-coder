# 2026-11-25 — Stack-tool reads go token-only; pushes stay paired

**Type:** Fix
Cross-package entry: [`docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md`](../../../../docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md)

The stack tools' GitHub client and `git_work`'s API client resolve token-only, so
`work_pr_read` and `work_item_assign_pr` answer through an identifier-less account.
`git_work` splits its token fetch: `project_token` for API calls, `project_push_token`
(paired, deriving) for the rebase push and `push_with_lease`'s push. `push_identity`
keeps both halves paired. Covered by ops-level `ReadPr`/`AssignPr` tests over a
vault-backed project row holding a metadata-less record.

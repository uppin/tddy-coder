# 2026-10-04 — Per-provider account control on the Projects screen

**Type:** Feature

Each project card shows one row per provider the vault has an account at, in one of three states
(assigned, no account assigned, unavailable on this host). The picker reads
`AccountsService.ListAccounts` and sends `SetProjectAccounts` with the full list. Cypress:
`ProjectAccountsAcceptance` 8/8, `ProjectsScreenAcceptance` 13/13. See
[projects-screen.md § Account assignment](../projects-screen.md#account-assignment).

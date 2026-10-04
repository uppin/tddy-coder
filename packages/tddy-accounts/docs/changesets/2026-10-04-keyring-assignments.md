# 2026-10-04 — Account resolution for a project

**Type:** Feature

`AccountResolution` (`Assigned`, `NotAssigned`, `UnknownOnThisHost`, `Ambiguous`) and the pure
`resolve_account` over a project's assignments and what the vault holds. An unassigned project
resolves to `NotAssigned` and to nothing else: no fallback to a caller's login or a sole vault
account. See [accounts-service.md § Resolving a project's account](../accounts-service.md#resolving-a-projects-account).

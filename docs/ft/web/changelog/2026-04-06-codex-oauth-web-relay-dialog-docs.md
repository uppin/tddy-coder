# 2026-04-06 — Codex OAuth web relay (dialog + docs)

- **tddy-web**: **`CodexOAuthDialog`** — modal (**`codex-oauth-dialog`**), dismiss (**`codex-oauth-dismiss`**), sandboxed authorize **iframe** when **`embeddingBlocked`** is false; **embedding-blocked** panel (**`codex-oauth-embedding-fallback`**) with external link (**`noopener`**, **`noreferrer`**) when **`embeddingBlocked`** is true. Cypress **`CodexOAuthDialog.cy.tsx`**, **`CodexOAuthIframeFallback.cy.tsx`**.
- **Docs**: **[codex-oauth-web-relay.md](../codex-oauth-web-relay.md)**; package **[codex-oauth-dialog.md](../../../../packages/tddy-web/docs/codex-oauth-dialog.md)**. Cross-package: **[docs/dev/changesets/](../../../dev/changesets/)**; **[packages/tddy-web/docs/changesets/](../../../../packages/tddy-web/docs/changesets/)**.

# 2026-10-04 — three `/accounts` components are over 60 lines

**Category:** Deferred from `#keyring` 4/9 `accounts` (#511)
**Source:** `/analyze-clean-code` at #511's wrap (score D)
**Consent:** the developer deferred the split until after the stack lands, 2026-10-04

| Component | File | Lines |
|---|---|---|
| `AccountRowView` | `packages/tddy-web/src/components/accounts/AccountsScreen.tsx` | ~65 |
| `renderOutcome` | `packages/tddy-web/src/components/accounts/AccountsScreen.tsx` | ~64 |
| `AccountsAppPage` | `packages/tddy-web/src/components/accounts/AccountsAppPage.tsx` | ~70 |

**Why deferred:** `#keyring` 8/9 (#515) edits both files, so a split here would turn its next
rebase into a conflict.

## What would close it

Once `#keyring` has landed, split each component by hand along its own seams: the row's view and
edit modes, one renderer per outcome, and the page's RPC calls apart from its rendering. Keep the
`data-testid`s, and have `AccountsScreenAcceptance.cy.tsx` green before and after. Delete this
entry when no component in `components/accounts/` is over 60 lines.

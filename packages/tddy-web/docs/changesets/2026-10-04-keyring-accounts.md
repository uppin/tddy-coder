# 2026-10-04 — The /accounts screen

**Type:** Feature

`#keyring` 4/9 — PR [#511](https://github.com/uppin/tddy-coder/pull/511). Cross-package entry:
[2026-10-04-keyring-accounts.md](../../../../docs/dev/changesets/2026-10-04-keyring-accounts.md).
Product entry: [2026-10-04-keyring-accounts.md](../../../../docs/ft/web/changelog/2026-10-04-keyring-accounts.md).

`ACCOUNTS_ROUTE = "/accounts"` and `isAccountsPath` (exact match); `AccountsAppPage` (one
`ListAccounts` per visit, rename and remove without a re-read, a failed action shown beside the list)
and the presentational `AccountsScreen` (four outcomes: empty, uninitialized, locked, error); a
`shell-menu-accounts` entry between Hosts and VMs; one route-chain branch in `src/index.tsx`;
generated `src/gen/accounts_pb.ts`. Not capability-gated. Detail:
[accounts-screen.md](../accounts-screen.md).

Tests: `AccountsScreenAcceptance.cy.tsx` (11) with the `accountsScreenPage` page object;
`appRoutes.test.ts` § accounts route (4); the menu-order lists in `ModelsNavAcceptance.cy.tsx` and
`PresenceCapabilityGatingAcceptance.cy.tsx` gain **Accounts**. Storybook: `AccountsScreen.stories.tsx`.

Component split deferred: `docs/dev/todo/2026-10-04-keyring-accounts-screen-long-components.md`.

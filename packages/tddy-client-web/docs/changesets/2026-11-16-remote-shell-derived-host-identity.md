# 2026-11-16 — Derived hosts are listed one parent at a time

**Type:** Fix · `#remote-shell` 9/11, PR [#925](https://github.com/uppin/tddy-coder/pull/925)

The derived host-directory source asks each connected parent on its own. `askParentForDerivedHosts(parent, sessionToken, deadlineMs?)` never rejects: a refused call, a `FAILED` outcome and a call past its deadline (`DERIVED_LISTING_DEADLINE_MS`, 20 s) are each that parent's failure, with the hosts that arrived. `mergeParentAnswer` folds one answer in — that parent's earlier hosts and failure are replaced, every other parent's are kept — and `parentsToAsk` selects the connected parents to ask, all of them again on a refresh. The `DerivedHostsRefresh` port (`DerivedHostsRefreshProvider`, `useRefreshDerivedHosts`, a no-op when no provider is mounted) lets a screen say a listing went stale; `useDerivedHostListingFailures` reads the per-parent failures for the New-session form. The all-at-once `listDerivedHosts` is removed.

Verification: `bun test` 156/156, including the five per-parent tests in `derivedSource.test.ts`; `tsc --noEmit` clean.

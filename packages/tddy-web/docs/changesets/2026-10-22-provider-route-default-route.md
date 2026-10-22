# 2026-10-22 — Default for… on a provider account row, and pre-selection on Create

**Type:** Feature

`#provider-route` 5/9 ([#666](https://github.com/uppin/tddy-coder/pull/666)).
`ProviderAccountDefaultsControl` on a provider-account row (keyed on the current defaults, so it
re-seeds when another row takes one), `AccountsAppPage` sending `SetProviderAccountDefaults` and
merging `changed`, `AccountRow.defaultFor`, and `useProviderDefault` pre-selecting the default in
`CreateSessionPane`. `ProviderModels` moved out of `AccountsScreen.tsx` into its own file
(486 → 455 lines). See [accounts screen](../accounts-screen.md) and
[create-session provider](../create-session-provider.md).

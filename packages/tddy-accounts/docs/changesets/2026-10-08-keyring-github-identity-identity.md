# 2026-10-08 — A project resolves to one acting GitHub identity

**Type:** Feature · `#keyring` 9/9, PR [#516](https://github.com/uppin/tddy-coder/pull/516)
Cross-package entry: [`docs/dev/changesets/2026-10-08-keyring-github-identity.md`](../../../../docs/dev/changesets/2026-10-08-keyring-github-identity.md)

`acting_identity(assignments, provider, held)` returns one `ActingIdentity { account, token, git }` from a
single `resolve_account`; the crate publishes no way to obtain either half alone. `IdentityError` has one
`Display` message per outcome (`NotAssigned`, `UnknownOnThisHost`, `Ambiguous`) and a fifth,
`Unusable`, for a record that resolves but carries no provider identifiers. The git identity derives from
`META_SUBJECT_ID` / `META_SUBJECT`, never from the mutable label.

Detail: [github-identity-resolution.md](../github-identity-resolution.md).

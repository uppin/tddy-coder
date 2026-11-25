# 2026-11-25 — A token-only resolution beside the paired one

**Type:** Fix
Cross-package entry: [`docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md`](../../../../docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md)

`acting_token(assignments, provider, held)` answers `ActingToken { account, token }`
from the record alone: the resolver's refusals, never `Unusable`, never the metadata.
`acting_identity` keeps its paired contract through the shared resolve-and-find step and
the shared `git_identity_of` address builder. `IdentityError::DerivationFailed` names the
account, the missing key, and why the provider would not supply it. `stamped_record`
writes derived identifiers onto a held record (account, label and secret preserved,
version bumped, `updated_at` set), persisted by
`SessionVaultAccountStore::stamp_account_identity`, which mints no record (`NotFound`
for an absent one).

Detail: [github-identity-resolution.md](../github-identity-resolution.md).

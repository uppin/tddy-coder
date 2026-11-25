# 2026-11-25 — The commit path derives a missing identity; reads take the token alone

**Type:** Fix · PR [#1044](https://github.com/uppin/tddy-coder/pull/1044)
Cross-package entry: [`docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md`](../../../../docs/dev/changesets/2026-11-25-accounts-identity-decoupled.md)

`SessionAccountAccess::acting_identity` derives on `Unusable`: the `AccountIdentityProbe`
(a closure, defaulting to the real `GET /user`, overridable via `with_user_probe`) asks
who the token belongs to, the answer is stamped back into the vault, and the paired
identity answers. Only a failed probe refuses, as `DerivationFailed`; a failed stamp is
warned about and the derived identity still answers. `acting_token` is the token-only
half, which never derives; `project_github_token` is its daemon-side form. New
workspace-internal edge on `tddy-github` for the fetch (acyclic — nothing in its tree
names this crate).

Detail: [session identity](../../../tddy-session-lifecycle/docs/session-identity.md).

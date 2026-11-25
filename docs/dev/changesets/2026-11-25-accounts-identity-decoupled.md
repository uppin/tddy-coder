# 2026-11-25 — Reads take the token alone; a missing commit identity is derived from it

**Type:** Fix
**Work item:** n9 (orchestrator queue)
**Packages:** `tddy-accounts`, `tddy-github`, `tddy-agent-launch`, `tddy-daemon-rpc`, `tddy-session-lifecycle`

## State A

`acting_identity` hands out a GitHub token only together with a `GitIdentity`, so an
account record with no `subject_id` — the operator's `uppin`, linked before the field
existed — is refused as `IdentityError::Unusable` for every GitHub interaction, reads
included. `work_pr_read` and `work_item_assign_pr` fail with "the github account uppin
has no `subject_id` recorded, so there is no identity to commit under".

## State B

- Two resolutions: `acting_token` answers the account and its token from the record
  alone for reads and PR-API calls (PR status, checks, comments, opening or updating
  a PR, assigning one); `acting_identity` stays paired for commits and pushes, so
  author and pusher still cannot diverge.
- A commit through an identifier-less record derives the missing half — `GET /user`
  with the account's token — and stamps it back into the vault record (version
  bumped, as a re-link is), so the next resolution and the next session start find
  it. Only a failed derivation refuses, as `IdentityError::DerivationFailed`, naming
  the account, the missing key, and GitHub's reason.
- No environment anywhere: no `GITHUB_TOKEN`/`GH_TOKEN`, no git config — before,
  during, or after.

## Caller classification

Stay paired (commit/push): `session_identity`, the pinned per-call token handler,
`project_push_token` (`realign`, `push_with_lease`), `push_identity`. Token-only
(read/API): `project_github_token` (`RepointWorkItem`), the stack tools' GitHub
client, `git_work`'s API client, trunk watch reads.

## Decisions taken (recorded for review)

1. **Derivation lives on `SessionAccountAccess::acting_identity`, not in the pure
   function.** The pure function cannot do I/O; every remaining paired caller is
   commit/push and wants derivation, so no call-site churn.
2. **The probe is a closure type** mirroring `SessionSubjectResolver`; production
   hits `https://api.github.com`, tests inject fakes.
3. **A successful probe whose persist fails still commits** (warn-logged): the
   identity is known-good, and refusing it would violate "refuse only if derivation
   fails".
4. **New edge `tddy-agent-launch → tddy-github`** (workspace-internal, acyclic): the
   curl transport and its headers live there and must not be duplicated.
5. **`realign`'s push token stays paired** per the commits-and-pushes line, and keeps
   `.ok()`: a refused token must not stop a push SSH could still carry.

## Backlog

Resolves and deletes
[`2026-10-24-github-account-missing-subject-id-no-commit-identity.md`](../todo/2026-10-24-github-account-missing-subject-id-no-commit-identity.md)
("A session starts with no commit identity when its GitHub account has no
`subject_id`"), by the backfill half of its "what would close it". Its alternative
(a re-link action on the account card) was not built.

Out of scope and unchanged: the separate "no credential vault is available to this
session start" problem (background starts carry no session token).

## Package entries

- [`tddy-accounts`](../../../packages/tddy-accounts/docs/changesets/2026-11-25-accounts-identity-decoupled.md)
- [`tddy-github`](../../../packages/tddy-github/docs/changesets/2026-11-25-accounts-identity-decoupled.md)
- [`tddy-agent-launch`](../../../packages/tddy-agent-launch/docs/changesets/2026-11-25-accounts-identity-decoupled.md)
- [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/docs/changesets/2026-11-25-accounts-identity-decoupled.md)
- [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/docs/changesets/2026-11-25-accounts-identity-decoupled.md)

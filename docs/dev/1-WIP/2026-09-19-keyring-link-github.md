# Changeset: Linking a second GitHub account

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#keyring` 8/9 · branch `feature/keyring/link-github` · base `feature/keyring/screen-share` (#514)

## Affected Packages

- **tddy-accounts** (new in `#keyring` 4/9) — the link flow, dedup, the removal refusal
- **tddy-service**: `proto/accounts.proto` — two RPCs, their messages, `LinkState`
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - the device flow reused **without** its session-minting half
- **tddy-web**: [capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md)
  - **Add account** on the Accounts screen; the session account marked

## Related Feature Documentation

- [PRD — Linking a second GitHub account](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-link-github.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [LiveKit and auth services](../../ft/daemon/auth-livekit-services.md)

## Summary

Adds a second GitHub account to the credential store **without replacing the session**. Login and
"having a GitHub credential" come apart: an account is a record, and exactly one record happens to be
the one the session was established with.

## Background

`ExchangeCode` returns `session_token`, `refresh_token` and a `GitHubUser` — completing it *is*
becoming that person. Running it again to add a second account signs the first one out. The
developer's requirement is *"multiple github accounts per session"*, so the flow that adds a
credential must not be the flow that establishes identity.

## Responsibility

**This node owns adding a credential without becoming its owner.**

- `BeginLinkAccount` / `PollLinkAccount` and their states;
- deduplication on the GitHub user id, and what a re-link preserves;
- the refusal to remove the account the vault's key derives from;
- the Accounts-screen control and the session-account marker.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| `BeginLinkAccount`, `PollLinkAccount`, `LinkState` | `tddy-service`, `tddy-accounts` |
| the link-session state and dedup rule | `tddy-accounts` |
| the `RemoveAccount` refusal for the session account | `tddy-accounts` |
| **Add account** + the session marker | `tddy-web` |

**Explicitly not this node's:**

- **Login** (2/9) — `ExchangeCode`, the device flow, `RefreshSession`, `Logout`. This node *reuses*
  2/9's token-exchange half and changes none of its behaviour.
- **The store** (3/9) — `tddy-credentials` is **unchanged**. A linked account is a record.
- **`AccountSummary`** (4/9) — reused with no new field, and still no secret.
- **Assignments** (5/9) and **propagation** (6/9) — a linked account is assignable and syncs for free.
- **Using the second account** (9/9) — resolving it for git and API operations is the next node's.
  This node ends when the record exists.

**The line this node must not cross**: **linking must never mint a session token.** If the
implementation finds that convenient — the device-flow code path already has one — that is the
boundary, not a shortcut. A link that silently re-identifies the caller is the bug the node exists to
prevent.

## Dependencies

**Parent in the line**: `#keyring` 7/9 `screen-share` — [#514](https://github.com/uppin/tddy-coder/pull/514).
**A line position, not a real edge.**

**The real edges**, both of them:

| Edge | PR | What this node takes |
|---|---|---|
| 2/9 `desktop-login` | [#509](https://github.com/uppin/tddy-coder/pull/509) | the OAuth App device flow, split so its token-exchange half runs without minting a session |
| 4/9 `accounts` | [#511](https://github.com/uppin/tddy-coder/pull/511) | `AccountsService`, `AccountSummary`, `RemoveAccount` and the screen |

3/9 is a transitive edge through 4/9 (the record model). 5/9 and 6/9 are **not** edges: this node
makes a linked account assignable and syncable by producing a record, not by touching either.

**Dependents**: 9/9 `github-identity` — it resolves a project's assigned account to a token, which
requires more than one account to exist for the resolution to be a choice.

**New external dependencies: none.**

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `accounts.proto`: `BeginLinkAccount`, `PollLinkAccount`, their messages and `LinkState`;
- `tddy-accounts`: the link-flow functions, signatures only, bodies `todo!()`;
- the `RemoveAccount` refusal variant.

**Failing tests**

- linking leaves `session_token` and the signed-in identity unchanged — the node's reason to exist;
- both accounts are listed; the session's own is marked;
- re-linking an existing account keeps its `account_id`, and a project assignment pointing at it
  still resolves;
- dedup is on the GitHub user id: the same id under a changed login name is one account;
- removing the session's own account is refused, with the reason;
- removing another linked account succeeds;
- a locked vault gives `LINK_VAULT_LOCKED`, never `LINK_DENIED`;
- denied and expired device flows are reported as themselves;
- `AccountSummary` carries no secret — asserted over the serialised response.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

### As published — measured red (wave 2, commit 2)

`./test -p tddy-accounts -p tddy-service -p tddy-github -p tddy-daemon-auth --no-fail-fast`,
**scoped to the four packages this node touches**. Whole-workspace green is CI's answer, not this
one's.

| | passed | failed |
|---|---|---|
| Before this commit (the inherited base) | 246 | 51 |
| After | 250 | 76 |

**+25 failing**, every one in this node's two new suites — 12 in `account_linking_unit.rs` and 13 in
`account_linking_acceptance.rs`. Each fails on a `todo!()`: 22 on this node's three
(`record_for_link`, `removal_allowed`, `begin_link_account`), and 3 on 4/9's unfinished
`list_accounts` / `remove_account`, which those tests read the result of. That is expected — 4/9 is a
real edge and is green before this node's own green phase starts.

**The 51 inherited failures are untouched**: 2/9–5/9's unfinished green phases, plus the four
`git_plumbing_shape.rs` assertions waiting on the `#carve` stack's move of `tddy-git` /
`tddy-github`.

**+4 passing, and they are green on purpose.** `packages/tddy-service/tests/linking_mints_no_session.rs`
asserts a property of the **schema**, and the schema is what this commit changes — so the tripwire is
satisfied the moment the surface lands, exactly as 7/9's two passphrase tripwires were. Recorded here
rather than manufactured into red. Non-vacuity was checked by adding `string session_token = 4;` to
`PollLinkAccountResponse` and confirming the assertion fails with that message.

The `tddy-web` spec `cypress/component/AccountLinkingAcceptance.cy.tsx` is 13 further failing tests,
run as the **single spec under change** — never the full Cypress suite.

### Design decisions taken while publishing the surface

**The session marker is `ListAccountsResponse.session_account`, a new `SessionAccount` message — not
a field on `AccountSummary`.** This keeps the `## Boundaries` promise that 4/9's `AccountSummary` is
reused with no new field, and it is the right shape besides: which account a session belongs to is a
fact about the *caller*, and 6/9 propagates the same record to a daemon where it is an ordinary
linked account. ⚠ It is still **one additive field on a message 4/9 owns**, and this node's green
phase must populate it inside `list_accounts` — a body 4/9 owns. Flagged for the reviewer; nothing
else in that message or its handler is touched here.

**A new `LinkedAccountStore` port rather than a `put` on 4/9's `AccountStore`.** 4/9 shaped
`AccountStore` as read-and-curate only (`list` / `set_label` / `remove`), which is what makes "no
store API returns a secret to a response path" checkable. Writing a *new* credential is a different
capability, so it gets its own port — `held` / `put` / `session_account` — following the
`ScreenSharingTargetStore` precedent 7/9 set.

**A daemon with no linking wired refuses, rather than pretending.** `AccountsServiceImpl::with_linking`
is additive, and `require_linking` answers `FAILED_PRECONDITION` when it was never called. **Not a
fallback**: nothing is substituted for the missing ports; the request does not happen, and says so.

⚠ **One file 4/9 owns was edited.** `tests/accounts_service_acceptance.rs` gained a single
`session_account: None,` in a `ListAccountsResponse` literal, which the new proto field made
incomplete. A forced mechanical consequence, not a change to 4/9's behaviour.

## Green wave

**Wave 4 of 5**, with 5/9, 6/9 and 7/9.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

**Intra-wave sort**: last in the wave. It has one dependent (9/9), which would normally put it ahead
of 6/9 and 7/9 — but 9/9 is in a later wave and so waits for the whole of wave 4 regardless, and this
node is the only one in the wave that touches **two** earlier nodes' surfaces (2/9's flow and 4/9's
service). Going last means both are settled when it starts. 5/9 leads the wave on the same
transitive-dependent count, ahead of this node because 9/9 needs the *resolver* before it needs a
second account to resolve to.

## Prerequisites

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

4/9 registers `AccountsService` there; this node adds RPCs to a service already registered and so
adds no lines to the recorded 806. Recorded because the node sits in that file's blast radius.

### ⚠ DURING — `build_auth_entries` complexity — [`complexity-auth-build-auth-entries`](../../../packages/tddy-daemon-auth/docs/code-issues/complexity-auth-build-auth-entries.md)

The recorded function is where `AuthService`'s RPC entries are assembled, and this node splits the
device flow that sits behind them. **Not claimed**: 2/9 [#509](https://github.com/uppin/tddy-coder/pull/509)
is the lowest node in this stack that touches the same function, and the lowest node that fixes an
entry is the one that claims it. This node adds no entries there — its two RPCs go on
`AccountsService`, not `AuthService`.

### Unanalyzed packages

`tddy-web` has no `docs/code-issues/`. **"Not measured" is not "clean"** — this node claims nothing
about it.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-link-github.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-link-github.md)
- [x] **Changeset**: this document
- [x] **Draft PR contract**: surface + failing tests (wave 2, commit 2)
- [ ] **Flow**: `BeginLinkAccount` / `PollLinkAccount` over 2/9's device flow, no session minted
- [ ] **Dedup**: on GitHub user id; re-link updates in place
- [ ] **Refusal**: the session's own account cannot be removed
- [ ] **UI**: Add account; the session account marked
- [ ] **Testing**: unit + acceptance, scoped
- [ ] **Package Documentation**: `packages/tddy-accounts/docs/account-linking.md`
- [ ] **Code Quality**: scoped clippy; CI green

## Technical Changes

### State A (Current)

- A GitHub credential enters the daemon **only** by logging in, and logging in replaces the session.
- One GitHub identity per daemon session; a second requires signing out.

### State B (Target)

- Any number of GitHub accounts as records; exactly one is the session's, marked as such.
- Linking requires an open vault and produces no session token.
- A re-link preserves `account_id`, so project assignments survive.

### Delta (What's Changing)

#### tddy-service
- **API**: `accounts.proto` gains `BeginLinkAccount`, `PollLinkAccount`, their messages and
  `LinkState { PENDING, LINKED, DENIED, EXPIRED, VAULT_LOCKED }`. **Additive** — nothing in
  `auth.proto` changes.

#### tddy-daemon-auth
- **Implementation**: 2/9's device flow splits into *obtain a token for a GitHub user* and *establish
  a session from one*. Login composes both; linking calls the first alone.

#### tddy-accounts
- **Implementation**: per-session link state keyed by `link_id`; dedup on GitHub user id; the
  `RemoveAccount` refusal.

#### tddy-web
- **UI**: **Add account** starts the flow, shows the user code and verification URI, and polls at the
  interval GitHub returns. The session's account carries a marker.

### Decisions recorded with their alternatives

**Refusing to remove the session's own account** rather than removing it and locking the vault. The
alternative is coherent — the KEK derives from that account, so losing it *is* a locked vault — and
it leaves a person holding a store they cannot open with no obvious route back. ⚠ Recorded so a
reviewer can overrule it.

**Dedup on the GitHub user id, not the login name.** A login name can be changed and re-registered by
someone else; a record keyed on it would eventually attach a stranger's token to a person's project.

**Same scopes as login.** A second account exists to act. Linking it read-only fails at push time,
far from the moment a person would connect the two.

## Implementation Milestones

- [ ] **M1** — split the device flow; login behaviour unchanged (test first)
- [ ] **M2** — `BeginLinkAccount` / `PollLinkAccount` and the link state
- [ ] **M3** — dedup on GitHub user id; re-link preserves `account_id`
- [ ] **M4** — the `RemoveAccount` refusal
- [ ] **M5** — Add account + the session marker
- [ ] **M6** — acceptance: two accounts, one session, assignments intact
- [ ] **M7** — `packages/tddy-accounts/docs/account-linking.md`

## Testing Plan

### Testing Strategy

**The load-bearing test is a negative one**: after a successful link, the caller's `session_token`
and identity are byte-for-byte what they were before. The convenient implementation — reuse the login
path and discard the token it returns — passes every *positive* test in this plan, and its first
symptom in production is a person silently acting as someone else.

Second in weight is the re-link test, because its failure is also silent: a fresh `account_id` leaves
projects unassigned, and 5/9's resolver reports `NotAssigned`, which is indistinguishable from never
having assigned one.

### Unit tests

- Dedup on GitHub user id, including a changed login name for the same id.
- A re-link updates the secret and keeps `account_id` and `label`, and moves `updated_at` to the
  moment of the link. ⚠ **Plan correction**: this list originally said `created_at`, which
  `CredentialRecord` does not have — the record carries `updated_at` and `version` only, and the
  moment an account was *first* linked is not retained anywhere. Nothing needs it today; if a screen
  ever does, it is a field on the record and a separate change.
- The `RemoveAccount` refusal fires for the session account and not for others.
- A locked vault maps to `LINK_VAULT_LOCKED`, not `LINK_DENIED`.

### Acceptance tests

- Linking leaves the session token and identity **unchanged**.
- Both accounts list; the session's own is marked.
- A project assigned to a re-linked account still resolves after the re-link.
- Denied and expired device flows are reported as themselves.
- `AccountSummary` carries no secret (over the serialised response).

### Verification scope

`./test -p tddy-accounts -p tddy-daemon-auth -p tddy-service` and scoped clippy. For `tddy-web`, the
single component spec under change — never the full Cypress run. Whole-workspace green comes from CI
via `scripts/ci-status.sh`.

## Acceptance Criteria

- [ ] Linking leaves `session_token` and the signed-in identity unchanged
- [ ] Both accounts appear; the session's own is marked
- [ ] Re-linking keeps `account_id` — project assignments survive
- [ ] Dedup is on the GitHub user id, not the login name
- [ ] Removing the session's own account is refused, with the reason
- [ ] Removing any other linked account succeeds
- [ ] A locked vault yields `LINK_VAULT_LOCKED`
- [ ] Denied and expired flows are reported as themselves
- [ ] `AccountSummary` still carries no secret
- [ ] Login, refresh and logout behave exactly as before

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [ ] M1–M7
- [ ] `packages/tddy-accounts/docs/account-linking.md`
- [ ] `/wrap-context-docs` — this node claims **no** `docs/dev/todo/` entry and **no** code-issue
      record

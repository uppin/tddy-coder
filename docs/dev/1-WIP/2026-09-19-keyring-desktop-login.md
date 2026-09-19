# Changeset: Zero-configuration GitHub sign-in on Tddy Desktop

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#keyring` 2/9 · branch `feature/keyring/desktop-login` · base `feature/keyring/signing-key` (#508)

## Affected Packages

- **tddy-github**: **no package documentation exists** — `#keyring` 1/9 creates
  `docs/session-token.md`; this node adds `docs/device-flow.md`
  - `src/provider.rs` — two device-flow methods on `GitHubOAuthProvider`
  - `src/real.rs` — the base-URL seam **first**, then the device flow and its polling state machine
  - `src/stub.rs` — stub implementations for the acceptance suites
- **tddy-service**: `proto/auth.proto` — `StartDeviceLogin` / `PollDeviceLogin`
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `src/auth.rs:109` — the `(Some(id), Some(secret))` gate; first-login enrolment
- **tddy-daemon-kernel**: [daemon-kernel.md](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md)
  - `src/config.rs` — persisting the enrolled `users:` entry. **Not split**
- **tddy-desktop**: [config-resolution-and-install.md](../../../packages/tddy-desktop/docs/config-resolution-and-install.md)
  - `desktop.yaml.production` — a public `client_id` by default, `client_secret` absent
- **tddy-web**: [capability-gating.md](../../../packages/tddy-web/docs/capability-gating.md)
  - the sign-in screen: show the user code, open the verification URI, poll, handle expiry/denial

## Related Feature Documentation

- [PRD — Zero-configuration GitHub sign-in on Tddy Desktop](../../ft/desktop/1-WIP/PRD-2026-09-19-keyring-desktop-login.md)
- [Tddy Desktop](../../ft/desktop/tddy-desktop-tauri.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [Daemon settings](../../ft/daemon/daemon-settings.md)

## Summary

Adds the **GitHub OAuth device flow** to the daemon — the flow the GitHub CLI uses, which
authenticates with a **public client id alone** — and makes a desktop deployment **enrol its first
successful login** against the OS user the application already runs as.

Together those remove the first and third of the three barriers `desktop.yaml.production:66-92`
records. `#keyring` 1/9 removed the second. After this node a fresh `./install --desktop` signs in
with no file edited by hand and no secret shipped.

## Background

Barrier 1 is `packages/tddy-daemon-auth/src/auth.rs:109`:

```rust
} else if let (Some(id), Some(secret)) = (&github.client_id, &github.client_secret) {
```

No secret → the `else` arm returns `AuthBuildResult { entries: vec![], … }`, so `auth.AuthService`
is never registered and the dashboard's `GetAuthUrl` answers `not_found`.

Barrier 3 is `packages/tddy-daemon-kernel/src/config.rs:1111`:

```rust
pub fn os_user_for_github(&self, github_user: &str) -> Option<&str> {
    self.users.iter().find(|u| u.github_user == github_user).map(|u| u.os_user.as_str())
}
```

An empty `users:` makes every login `None` → `permission_denied: user not mapped to OS user`,
deliberately and with no fallback.

**No device-flow code exists in the tree.** `real.rs` holds exactly one OAuth path: `authorize_url`
at `:45` and the redirect-flow `exchange_code` at `:59`, which posts `client_secret`.

## Responsibility

**This node owns how a desktop user proves who they are to their own daemon, without configuration.**

- the device-flow half of `GitHubOAuthProvider` and its one real implementation;
- the `StartDeviceLogin` / `PollDeviceLogin` RPCs and their polling state machine;
- the `auth.rs:109` gate — what a `client_id` without a secret is allowed to register;
- **first-login enrolment**: the one-time write of a `users:` row on a deployment that has none;
- the base-URL seam in `RealGitHubProvider`, which this node needs before it doubles the crate's
  network surface.

## Boundaries

**Owned surface:**

| Symbol | Crate |
|---|---|
| `GitHubOAuthProvider::start_device_login` / `::poll_device_login` | `tddy-github` |
| `DeviceLoginStart`, `DeviceLoginPoll` (the state enum) | `tddy-github` |
| `RealGitHubProvider::new_with_base_urls` (the seam) | `tddy-github` |
| `auth.StartDeviceLogin` / `auth.PollDeviceLogin` | `tddy-service` proto |
| first-login enrolment and its persistence | `tddy-daemon-auth` + `tddy-daemon-kernel` |

**Explicitly not this node's:**

- The session token itself — format, signing and verification are `#keyring` 1/9's, and this node
  mints the *same* token by a different route.
- Where the GitHub access token is stored — `#keyring` 3/9 replaces `FileGitHubTokenStore`. This
  node writes through whatever store exists at its base.
- **A second GitHub account.** This node *refuses* one, which is correct until `#keyring` 8/9 builds
  the deliberate path to add it.
- Splitting `auth.rs` (1,200 lines) or `config.rs` (1,447 production lines) — no restructure in this
  stack, per two independent records.

**The line this node must not cross.** `os_user_for_github` gains **no default arm and no fallback**.
Enrolment writes the row the existing lookup then finds; the lookup's behaviour for an unmapped
login is unchanged, and a second, different login on an enrolled deployment is refused exactly as
today. A fallback would answer "no mapping" *every time*, which would let any GitHub account on
earth drive the desktop user's machine — the unsafe shape CLAUDE.md forbids, and the reason this is
framed as one-time enrolment instead.

## Dependencies

**Parent**: `#keyring` 1/9 `signing-key` — [#508](https://github.com/uppin/tddy-coder/pull/508).

Two things are taken from it, both load-bearing:

- **barrier 2 is gone**: a daemon with no `livekit:` block has a working signer, so there is
  something to mint a session *with* once GitHub answers;
- the `v2` token and its `kid`, which this node's flow produces unchanged.

**Dependent**: `#keyring` 8/9 `link-github` — the deliberate second account this node refuses.

**New external dependency**: none in the shipped build. Two HTTP calls to endpoints the crate
already talks to, and `tokio::time` for the poll interval.

> **One dev-dependency added**, recorded rather than slipped in: `axum = "0.8"` on `tddy-github`,
> **`[dev-dependencies]` only**, to serve a GitHub on loopback. It is already this workspace's HTTP
> server in five crates and already in `Cargo.lock`, so it resolves nothing new and ships nothing.
> Without it the device flow's four non-error states are unreachable by any test.

**Two answers owed before the green phase**, recorded rather than assumed (PRD § *Technical Impact*):
whether the app registers as an **OAuth App** (recommended — its user token does not expire, so no
refresh exchange ever needs a secret) or a GitHub App (whose 8-hour token's refresh exchange *does*
post `client_secret`, re-introducing barrier 1 on day two); and whether the public client id is
**rendered into `desktop.yaml.production`** (recommended) or compiled into the binary.

## Draft PR contract

Published in this PR's **second commit**, before implementation:

**Surface**

- `tddy-github` — `GitHubOAuthProvider::start_device_login() -> Result<DeviceLoginStart, String>`
  and `::poll_device_login(&str) -> Result<DeviceLoginPoll, String>`, with
  `DeviceLoginPoll { Pending, SlowDown, Denied, Expired, Complete(String, GitHubUser) }`;
  `RealGitHubProvider::new_with_base_urls(..)`. Bodies `todo!()`.
- `tddy-service` — `StartDeviceLogin` / `PollDeviceLogin` RPCs and messages in `auth.proto`.

**Failing tests**

- acceptance: a daemon configured with **`client_id` only** registers `auth.AuthService` and serves
  `StartDeviceLogin`;
- acceptance: poll → poll → approval yields a `v2` session token and refresh token in the same shape
  `ExchangeCode` returns;
- acceptance: on an empty `users:`, the first login is enrolled and persisted; a second, different
  login is refused `permission_denied`;
- unit: `slow_down` widens the interval; `expired_token` and `access_denied` are distinct states;
- unit: no request in the device flow carries a client secret;
- unit: `RealGitHubProvider` against a local base URL exercises `exchange_code`'s error returns.

  > **Measured correction.** `exchange_code` has **seven** error returns, not six: the forged
  > state, two transport failures, two non-success statuses and two parse failures.
  > `tests/real_provider_over_http.rs` covers six of them. The seventh — a transport failure on the
  > *user* leg specifically, after the token leg succeeded — needs the stand-in to die between two
  > requests, and is left to `/green` rather than claimed.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

## Green wave

**Wave 2 of 5**, with `#keyring` 3/9. Its only unmet need is wave 1.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

⚠ **Declared deviation.** Inside wave 2 the sort puts blockers first, and `#keyring` 3/9 has six
transitive dependents to this node's one — so the sort would place 3/9 ahead. This node is first by
the developer's explicit instruction that "one of the 1st PRs should also be focused on unblocking a
GitHub login local experience on the tddy-desktop setup". Recorded so the order is not mistaken for
the sort's output. The cost is that 3/9 lands one PR later than it otherwise would; nothing
downstream changes, because both are wave 2 and neither depends on the other.

## Prerequisites

### ✅ RESOLVED HERE — a desktop install configures no identity — [`2026-09-18-desktop-install-configures-no-identity.md`](../todo/2026-09-18-desktop-install-configures-no-identity.md)

The entry is this node's problem statement: `./install --desktop` renders `github:`, `livekit:` and
`users:` unset, so the installed app opens on its settings screen with no sessions. `#keyring` 1/9
closed the `livekit:` half; this node closes the other two — a public `client_id` in
`desktop.yaml.production` with no secret, and first-login enrolment in place of a hand-written
`users:` row.

Closed when a fresh `./install --desktop` reaches a signed-in dashboard with no file edited by hand.
This node's wrap deletes the entry.

### ⚠ DURING — models/agents open items at wrap — [`2026-08-16-models-agents-open-items-at-wrap.md`](../todo/2026-08-16-models-agents-open-items-at-wrap.md)

Touches this node's config surface. Recorded, not fixed.

### ⚠ DURING — Tauri desktop single-process daemon — [`2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md)

The application's own process **is** the daemon, which is why enrolment can know the running OS user
without asking anything. The entry's open items are unrelated to this node and stay open.

### ⚠ DURING — `build_auth_entries` complexity — [`complexity-auth-build-auth-entries`](../../../packages/tddy-daemon-auth/docs/code-issues/complexity-auth-build-auth-entries.md)

104 lines spanning both `:68` and `:109`. This node edits the second gate and `#keyring` 1/9 edits
the first. Both shrink it; neither restructures it. Recorded, not claimed.

### ✅ RESOLVED HERE — `RealGitHubProvider::exchange_code` is untestable by construction — [`missing-tests-real-exchange-code`](../../../packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md)

Opened 2026-09-19 by the `/analyze-code-issues` pass that landed on `#keyring` 1/9. Two hardcoded
absolute hosts (`real.rs:67`, `:91`), no base-URL field on the struct (`:11`) or constructor
(`:33`), 0 of 6 error returns exercised, 0 tests entering the function.

**This node claims it** because it is the node that makes it worse otherwise: the device flow adds
roughly another hundred lines of network code to the same provider, plus a polling state machine
that is harder to get right than a two-call exchange. Opening the seam first is cheaper than opening
it twice. Closed when the provider takes a base URL and the six error returns are covered.

⚠ **Re-run note carried from the record**: `AuthServiceImpl::exchange_code` is a *different*
function on the service, over `StubGitHubProvider`, with six tests. It shares only the name. Any
later measurement must distinguish them by receiver type.

### Unanalyzed packages

`tddy-desktop` and `tddy-web` have no `docs/code-issues/` directory — `/analyze-code-issues` has
never run there, which is **not** a clean bill of health. Named, not blocking; this node does not
open them, because its own `tddy-github` finding is the one in its path.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-desktop-login.md](../../ft/desktop/1-WIP/PRD-2026-09-19-keyring-desktop-login.md)
- [x] **Changeset**: this document
- [ ] **Draft PR contract**: owned surface + failing tests (wave 2, commit 2)
- [ ] **Base-URL seam** in `RealGitHubProvider`, with the six error returns covered — **before** the device flow
- [ ] **Implementation**: device flow, RPCs, the gate, enrolment, the web sign-in screen
- [ ] **Decisions answered**: OAuth App vs GitHub App, verified against the live API; client-id placement
- [ ] **Testing**: acceptance + unit; the single `tddy-web` spec under change
- [ ] **Package Documentation**: the six packages above
- [ ] **Code Quality**: scoped clippy per package; CI green

## Technical Changes

### State A (Current)

- `auth.rs:109` registers a real provider only for `(Some(client_id), Some(client_secret))`;
  otherwise no auth entry at all.
- `GitHubOAuthProvider` has three methods — `authorize_url`, `exchange_code`,
  `issues_usable_access_token` — all shaped around a redirect and an authorization code.
- `real.rs` posts `client_secret` to a hardcoded `https://github.com/login/oauth/access_token`, then
  fetches a hardcoded `https://api.github.com/user`.
- `users: []` on a desktop install → every login `permission_denied`.
- `auth.proto` has five RPCs; none is a device flow.

### State B (Target)

- `client_id` alone registers a **device-flow** provider; `client_id` + `client_secret` continues to
  register the redirect-flow provider unchanged. Two configurations, neither a fallback for the other.
- `GitHubOAuthProvider` has five methods; `RealGitHubProvider` holds a base URL pair and is testable
  against a local server.
- `auth.proto` has seven RPCs. `PollDeviceLogin` returns the same triple `ExchangeCode` returns.
- A desktop deployment with an empty `users:` **enrols its first successful login** against the
  running OS user, persists it, and refuses every different login afterwards.

### Delta (What's Changing)

#### tddy-github
- **API**: `start_device_login` / `poll_device_login` on the trait; `DeviceLoginStart`,
  `DeviceLoginPoll`; `RealGitHubProvider::new_with_base_urls`.
- **Implementation**: the seam first; then the start call and the polling state machine honouring
  `interval`, `slow_down`, `expired_token`, `access_denied`.
- **Tests**: the six previously-unexercised error returns in `exchange_code`.

#### tddy-service
- **API**: `StartDeviceLogin` / `PollDeviceLogin` and their messages in `auth.proto`.

#### tddy-daemon-auth
- **Implementation**: the `:109` gate; two handlers; first-login enrolment and the refusal path.

#### tddy-daemon-kernel
- **Implementation**: persisting the enrolled `users:` row. `os_user_for_github` **unchanged**.

#### tddy-desktop
- **Configuration**: a public `client_id` rendered into `desktop.yaml.production`; no secret.

#### tddy-web
- **Integration**: the device-flow sign-in screen — user code, verification URI, poll, expiry,
  denial.

## Implementation Milestones

- [ ] **M1** — base-URL seam in `RealGitHubProvider`; six error returns covered
- [ ] **M2** — trait methods + `StubGitHubProvider`
- [ ] **M3** — `auth.proto` RPCs and messages, regenerated
- [ ] **M4** — device flow and polling state machine in `real.rs`
- [ ] **M5** — the `:109` gate
- [ ] **M6** — first-login enrolment, persistence, and the refusal path
- [ ] **M7** — `tddy-web` sign-in screen
- [ ] **M8** — `desktop.yaml.production` and the `tddy-desktop` docs that state the barriers

## Testing Plan

### Testing Strategy

**Primary level: acceptance.** The claim is about a *deployment* — "a daemon configured this way
registers that service and signs this user in" — which no unit test can express. Unit tests take the
polling state machine and the provider's error paths, where a single process is the honest scope.

### Acceptance tests

- **Device-flow sign-in end to end** — Given a daemon with `client_id` and **no** `client_secret`,
  When the client starts a device login and the code is approved, Then a `v2` session token and a
  refresh token are returned. *Assertions*: `auth.AuthService` is in the registered service list; the
  returned token authenticates a subsequent token-gated RPC; **no request carried a client secret**.
- **First-login enrolment** — Given `users: []`, When login A succeeds, Then a `users:` row binding A
  to the running OS user is persisted. *Assertion*: re-reading the config finds exactly one row.
- **Enrolment is once, not a fallback** — Given an enrolled deployment, When a *different* GitHub
  login B completes the device flow, Then it is refused `permission_denied: user not mapped to OS
  user`. *Assertion*: the persisted `users:` still holds exactly one row, A's.
- **The redirect flow is untouched** — Given `client_id` + `client_secret`, When a client calls
  `GetAuthUrl` / `ExchangeCode`, Then behaviour is byte-identical to today.

### Unit / integration tests

- `slow_down` widens the poll interval; `expired_token` and `access_denied` are distinct states.
- A pending poll does not mint a token.
- `RealGitHubProvider` against a local base URL: all six `exchange_code` error returns.
- `os_user_for_github` is unchanged — an unmapped login is still `None`.

### tddy-web

The device-flow sign-in spec only. **Never a full `cypress:e2e` run** — ~50 minutes over 207 specs —
and `tsc` is not a gate in this repo. JS dependencies install through the local registry
(`./dev bun run local-registry-install`), never plain `bun install`.

### Verification scope

`./test -p tddy-github -p tddy-daemon-auth -p tddy-daemon-kernel -p tddy-service`, scoped clippy per
package, and the single web spec. Whole-workspace green comes from CI.

## Acceptance Criteria

- [ ] A daemon configured with **`client_id` only** registers `auth.AuthService` and serves `StartDeviceLogin`
- [ ] `StartDeviceLogin` returns a `user_code` and `verification_uri`, and no client secret is sent
- [ ] `PollDeviceLogin` returns `pending` until approval, then the same triple `ExchangeCode` returns
- [ ] `slow_down`, `expired_token` and `access_denied` are honoured as distinct states
- [ ] The **first** login on an empty `users:` is enrolled against the running OS user and persisted
- [ ] A **second, different** login on an enrolled deployment is refused — no fallback
- [ ] `os_user_for_github` is unchanged and has no default arm
- [ ] `client_id` + `client_secret` still serves the redirect flow unchanged
- [ ] `RealGitHubProvider` takes a base URL; `exchange_code`'s six error returns are covered
- [ ] A fresh `./install --desktop` reaches a signed-in dashboard with **no file edited by hand**

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Publish the draft-PR contract — wave 2
- [ ] M1 — the seam (do this first)
- [ ] M2–M8
- [ ] Answer the OAuth App / GitHub App question against the live API
- [ ] Decide and record the client-id placement
- [ ] Package documentation for the six affected packages
- [ ] `/wrap-context-docs` — deletes `2026-09-18-desktop-install-configures-no-identity.md` and the
      `missing-tests-real-exchange-code` record this node claims

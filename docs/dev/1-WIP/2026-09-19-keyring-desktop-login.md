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
- **V1 — transport-stamped request metadata** (see *Decisions during green*):
  - **tddy-rpc**: `RequestTransport`, `RequestMetadata::over` (no `Default`), `Request::direct`
    (no `Request::new`), `ServerEngine::new(service, transport)`, `start_bidi_stream(.., metadata, ..)`
  - **tddy-tauri-rpc**, **tddy-livekit**, **tddy-stdio**, **tddy-connectrpc**, **tddy-codegen**,
    **tddy-service** (tonic supplement): the stamping hosts
  - **tddy-github** (`LoginAdmission::admit`), **tddy-daemon-auth** (`FirstLoginEnrolment`)
  - every crate that built a request or metadata — `Request::direct` / an explicit transport at each
    site; no behaviour change there

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
- [x] **Draft PR contract**: owned surface + failing tests (wave 2, commit 2) — `9279fb17`
- [x] **Base-URL seam** in `RealGitHubProvider`, with the six error returns covered — **before** the device flow (`604c4994`; all **seven** covered, the user-leg one included)
- [~] **Implementation**: device flow, RPCs, the gate, enrolment, the web sign-in screen — done (M1–M7); `desktop.yaml.production` (M8) pending
- [~] **Decisions answered**: **OAuth App** (developer, during green) — ⚠ not yet verified against the live API; client id **rendered into `desktop.yaml.production`** — decided, the id itself is still owed by the developer (M8)
- [~] **Testing**: acceptance + unit written for every item; the `tddy-web` spec passes 22/22 — ⚠ the scoped Rust run was blocked by a full disk at validation (see *Validation Results*)
- [ ] **Package Documentation**: the six packages above
- [ ] **Code Quality**: scoped clippy per package; CI green — ⚠ clippy blocked by a full disk at validation; builds clean with no warnings

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
- **Implementation**: persisting the enrolled `users:` row. `users:` becomes a shared live holder
  (`LiveUsers`) carried by `DaemonConfig`, so an enrolled row is seen by every service without a
  restart. `os_user_for_github`'s **behaviour is unchanged** (no default arm, unmapped → `None`); its
  return type becomes `Option<String>`, since a borrow cannot escape the lock.

#### tddy-desktop
- **Configuration**: a public `client_id` rendered into `desktop.yaml.production`; no secret.
  - 🔲 **M8, pending** — the file is unchanged at `HEAD` and still documents `client_id` +
    `client_secret` (`desktop.yaml.production:90-92`); waiting on the OAuth App's client id.

#### tddy-web
- **Integration**: the device-flow sign-in screen — user code, verification URI, poll, expiry,
  denial.
  - [x] Written (red): `packages/tddy-web/cypress/component/DeviceLoginAcceptance.cy.tsx` — 22
    tests. Pins `DeviceLoginPanel` (`src/components/DeviceLoginPanel.tsx`), `DaemonLoginScreen`
    extracted from `src/index.tsx` to `src/components/DaemonLoginScreen.tsx` with an `authFlow`
    prop, and `ClientConfig.authFlow` read from `/api/config`'s `auth_flow`.
  - [x] Implemented (green, `20ec471e`). **Decision (developer):** `auth_flow`
    (`"device"` | `"redirect"`) is declared on **both** config paths — `/api/config`
    (`server.rs`, `main.rs`) and `GetClientConfigResponse.auth_flow` (`daemon_config.proto` field
    10, `daemon_config_service.rs`) — both from the one `github_auth_flow` decision in `auth.rs`.
  - [x] **Absent `auth_flow` is "no sign-in configured"**, never the redirect flow: the screen says
    so and offers neither flow. An unrecognised value is an error the screen names. Both readers
    (`clientConfig.ts` `authFlowOf`) always state the declaration (`AuthFlowDeclaration`).

## Decisions during green

- **No fallbacks, no backward compatibility (developer, 2026-09-23):** "don't leave any
  fallbacks. I'll rollout all the desktop instances." Every desktop is rolled out together, so
  nothing here keeps a compatibility path for an older daemon or dashboard. Applied as:
  - the dashboard reads a missing `auth_flow` as "this daemon has no GitHub sign-in configured"
    and an unknown one as an error — absence no longer offers the redirect button;
  - a public client refuses `GetAuthUrl` (`failed_precondition`, naming the device flow), as it
    already refused `ExchangeCode`; `GitHubOAuthProvider::authorize_url` returns `Result`;
  - `poll_device_login` errors on a `slow_down` with no interval for a code it has no open attempt
    for, instead of widening GitHub's documented 5 s default;
  - the client ends a device attempt `failed` on a grant or `SLOW_DOWN` without a positive
    interval, and on a `COMPLETE` missing its user or tokens;
  - `runtime::build` refuses an embedded host that serves sign-in to an empty `users:` but names
    no config file, instead of silently assembling a desktop that can never enrol.

- **V1 — transport-stamped request metadata (developer, 2026-09-23):** option 2 of *V1 options*.
  Enrolment is decided by how the completing call reached the daemon, stamped by the host that
  received it, never by anything the caller wrote. Applied as:
  - `tddy_rpc::RequestTransport { InProcess, LiveKit, UnixSocket, Pipe, Http, Grpc, Direct }`, and
    `RequestMetadata.transport` (private field, read through `transport()`). `Direct` is a call one
    component makes on another in code, received over nothing.
  - **No default metadata.** `RequestMetadata` no longer derives `Default` and can only be built
    through `RequestMetadata::over(transport)`; `Request::new(inner)` is gone — a request is either
    stamped by its host (`with_metadata`, `from_rpc_message`) or says `Request::direct(inner)`. Every
    construction site, tests included, names its transport (~590 `Request::new` → `Request::direct`,
    ~90 metadata sites).
  - **Hosts stamp, envelopes cannot.** `ServerEngine::new(service, transport)` takes the transport
    once from its host and stamps every dispatched message with it; the envelope's
    `sender_identity` and `metadata` map (the fields a sender writes) are never read for it, and the
    proto has no transport field. A generic endpoint that cannot tell its channel's kind
    (`StdioEndpoint::from_duplex`) takes the transport from whoever opened the channel.
  - `RpcService::start_bidi_stream` gains a `metadata: RequestMetadata` parameter, so a bidi
    handler's `Request` carries its session's stamp instead of a false one (the generated handler
    used to build it with `Request::new`).
  - `LoginAdmission::admit(github_login, transport)`; `complete_login` takes the transport, read by
    both `exchange_code` and `poll_device_login` before `into_inner`. `FirstLoginEnrolment` matches
    the transport **exhaustively** — only `InProcess` enrols; any other on an unenrolled desktop logs,
    writes nothing and admits the login unmapped (its RPCs refused `permission_denied`). A transport
    added later must be decided there. A server has no admission and is unaffected.
  - **Forwarding** needs no special case: `forward_to_peer` sends only the request bytes, and the
    receiving daemon's `LiveKitParticipant` stamps it `LiveKit`. A login forwarded from a desktop's
    window is, on the peer, a login from the room. Pinned by
    `tddy-daemon-livekit/tests/forwarded_rpc_is_stamped_by_the_receiver.rs`.

### Transports and their stamping sites

| Transport | Host | Stamping site |
|---|---|---|
| `InProcess` | Tauri IPC — `WebviewRpcHost`, `MultiConnectionHost` (the desktop's window) | `tddy-tauri-rpc/src/host.rs:95`, `src/multi_host.rs:120` |
| `LiveKit` | `LiveKitParticipant::connect` / `::join` — the common room, session rooms, peer forwards | `tddy-livekit/src/participant.rs:338`, `:495` |
| `UnixSocket` | `StdioEndpoint::from_duplex` over a Unix socket: agent tool socket, sandbox tool socket, toolcall listener, supervisor socket, host-session socket | `tddy-daemon/src/agent_tool_socket.rs:71`; `tddy-sandbox-runner/src/runner.rs:1699`; `tddy-sandbox-app/src/sandboxed_session.rs:688`; `tddy-toolcall/src/toolcall/listener.rs:200`; `tddy-session-lifecycle/src/session_toolcall.rs:123`; `…/connection_service/svc_start_claude_cli_session.rs:224`; `tddy-supervisor/src/server.rs:596`; `tddy-coder/src/run.rs:1940`; clients (host a no-callback service): `tddy-session-tool-client/src/lib.rs:777`, `tddy-toolcall/src/toolcall/client.rs:73`, `tddy-supervisor/src/client.rs:68` |
| `Pipe` | a parent/child's stdio: `StdioEndpoint::from_process_stdio`, `from_child_stdio`; a jail's piped stdio | `tddy-stdio/src/endpoint.rs:87`, `:120`; `tddy-daemon-sandbox/src/sandbox_session.rs:210` |
| `Http` | Connect-RPC `/rpc` router | `tddy-connectrpc/src/router.rs:182` (`over_http`) |
| `Grpc` | tonic: codegen'd `*TonicAdapter`, the exec-tool supplement, `From<tonic::Request>` | `tddy-codegen/src/generator.rs:1081`, `:1094`; `tddy-service/exec_tool_tonic_adapter_supplement.rs:32,55,72,88`; `tddy-rpc/src/types.rs:114` |
| `Direct` | none — `Request::direct`, a call in code | every in-process delegation (e.g. `daemon_rpc_handler.rs`, `token_service.rs`) and handler-level tests |

The engine stamps at `tddy-rpc/src/server_engine.rs` `metadata_of` (unary, client-streaming
fragments, bidi open and continuations, and the bidi session's own metadata).

## Implementation Milestones

- [x] **M1** — base-URL seam in `RealGitHubProvider`; six error returns covered
- [x] **M2** — trait methods + `StubGitHubProvider`
- [x] **M3** — `auth.proto` RPCs and messages, regenerated
- [x] **M4** — device flow and polling state machine in `real.rs`
- [x] **M5** — the `:109` gate
- [x] **M6** — first-login enrolment, persistence, and the refusal path
- [x] **M7** — `tddy-web` sign-in screen
- [ ] **M8** — `desktop.yaml.production` and the `tddy-desktop` docs that state the barriers — 🔲 pending the client id from the developer

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

## Refactoring Needed

### From @red (TDD Red Phase)

- `cypress/support/rpc/deviceLoginBackend.ts` reuses `CURRENT_ACCESS_TOKEN` / `VALID_REFRESH_TOKEN`
  from `durableSessionBackend.ts`, which mints `v1.` tokens; the daemon now mints `v2`. Harmless for
  the client, which only decodes `exp`, but a shared `v2` fixture belongs in one place.
- The poll-timing tests flush one real macrotask before every `cy.tick` (`letTheAnswerInFlightLand`),
  because a poll is recorded when sent and answered a few promise hops later. Any other
  `cy.clock`-driven polling spec over the in-memory transport needs the same helper; it belongs in
  `cypress/support/` once a second spec wants it.

### From @validate-changes (2026-09-23)

- ~~**Enrolment is reachable from the LiveKit common room.**~~ — done (V1): transport-stamped
  request metadata; only an in-process login enrols.
- ~~Bound `RealGitHubProvider::device_poll_intervals`~~ — done (V4).
- ~~Floor the client's poll interval~~ — done as a protocol error rather than a floor (V5).
- `admit` does synchronous file I/O under a `std::sync::Mutex` on an async RPC task
  (`first_login_admission.rs:43`, `live_users.rs` `enrol_first_login`). It happens once per
  deployment, so this is acceptable, but `spawn_blocking` would be the tidy shape.

## Validation Results

**Run:** `/validate-changes`, 2026-09-23, `pr-509-green` @ `bba454da` (= `origin/feature/keyring/desktop-login`).

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes, planned (base `origin/feature/keyring/signing-key`, #508) |
| `/pr-stack-rebase` | ✅ Already current: the base tip is an ancestor of `HEAD` |
| Leak check (`origin/feature/keyring/signing-key..HEAD`) | ✅ Clean: exactly this PR's 7 commits (`f45d3bc3` … `bba454da`) |
| Diff contains only this PR's files | ✅ 64 files, all claimed by an item here (the `os_user_for_github` return-type ripple included) |
| Parent-owned files intact | ✅ No deletions; `signing_key.rs` and `session_token_v2.rs` untouched |

### Stack boundary

| Check | Result |
|---|---|
| Changeset items implemented or deferred | ⚠ 1 open: M8 (`desktop.yaml.production`), deferred for the developer's client id |
| `## Responsibility` delivered | ✅ No stubs. The only new `TODO` is recorded (`first_login_enrolment.rs:84`, comments lost on rewrite) |
| `## Dependencies` not implemented here | ✅ Clean. Consumes #508's `SessionTokens`, `load_signing_key`, `build_auth_entries_with` and the `v2` signer without editing them |
| `## Boundaries` respected | ✅ No second-account path; `os_user_for_github` has no default arm; `auth.rs` and `config.rs` not split |
| No dependent's behaviour | ✅ Clean. Nothing from 3/9 (token store) or 8/9 (link-github) |

### Build and tests (scoped)

| Package | Build | Tests |
|---|---|---|
| tddy-github | ✅ clean, no warnings | ⚠ not run: disk full (see below) |
| tddy-service | ✅ clean | ⚠ not run |
| tddy-daemon-kernel | ✅ clean | ⚠ not run |
| tddy-daemon-auth | ✅ clean | ⚠ not run |
| tddy-daemon | ✅ clean | ⚠ not run (targeted: `first_login_enrolment_acceptance`, `server_options_acceptance`, `daemon_config_service`, `test_placement`) |
| tddy-coder, tddy-host-service, tddy-worktree-service, tddy-telegram, tddy-session-lifecycle | ✅ clean (the return-type ripple compiles) | not run. The known session-lifecycle sandbox red belongs to the parent, not this PR |
| tddy-web `DeviceLoginAcceptance.cy.tsx` | n/a | ✅ **22/22** passing |

⚠ **Blocked:** the machine's Data volume filled up during the scoped `cargo test` / `cargo clippy`
run (407 MiB free, `ENOSPC` creating `target/debug/.fingerprint`). Build artifacts were not deleted
without consent. Re-run after freeing space:
`./test -p tddy-github -p tddy-daemon-auth -p tddy-daemon-kernel -p tddy-service`, the four targeted
`tddy-daemon` suites, and `cargo clippy -p <pkg> --all-targets -- -D warnings` for each. Or read CI.

### Risks

| # | Severity | Where | Finding |
|---|---|---|---|
| V1 | ✅ Resolved (was 🔴 High, security) | `runtime.rs:1467-1470`, `runtime.rs:584`, `first_login_admission.rs:43` | An embedded daemon serves **every** entry, `auth.AuthService` included, on the LiveKit common room, and it attaches enrolment to that same service. On an **unenrolled** desktop that has a `livekit:` block, any room peer can run `StartDeviceLogin` / `PollDeviceLogin` with its own GitHub account. It then becomes the one enrolled operator, mapped to the desktop's OS user. A fresh install has no `livekit:`, but an install that got past barrier 2 and stalled at barrier 3 has exactly that shape. Proposal: enrol only for a login that arrives over the in-process (Tauri) bridge, or refuse enrolment while the common room is served. No test covers either transport. **Resolved (green, option 2 — developer's choice):** every serving host stamps `RequestMetadata`'s transport itself, and `FirstLoginEnrolment` enrols only a login completed `InProcess`; a login over the common room or the agent tool socket is minted unmapped and writes nothing. See *Decisions during green* and *Transports and their stamping sites*. Pinned end to end over the real `LiveKitParticipant` and the runtime's own agent tool socket (`first_login_enrolment_acceptance`, four new tests). |
| V2 | 🟠 Medium | `first_login_enrolment.rs:84` | The first login rewrites `~/.tddy/desktop.yaml` via `serde_yaml::Value` and **strips every comment**, including the whole explanatory header `desktop.yaml.production` renders. Recorded as a TODO, but it hits every desktop on its first run. |
| V3 | 🟠 Medium | `desktop.yaml.production:90-92` | M8 is not done, so the last acceptance criterion (a fresh install signs in with no edit) is unmet, and the file still tells operators to add a `client_secret`. |
| V4 | ✅ Resolved | `real.rs` `device_attempts` | `device_poll_intervals` gains an entry for every started (or slowed-down) device code and loses it only on a terminal answer, so abandoned attempts accumulate. `StartDeviceLogin` is unauthenticated, though every entry costs a successful GitHub call. **Resolved (green):** each entry records its code's `expires_in` deadline (`DeviceAttempt`); every start and poll prunes closed windows (`open_device_attempts`), and Complete / Denied / Expired / any other error remove the entry. Pinned by `a_device_code_is_forgotten_once_its_window_has_passed` and `…_once_github_answers_it_expired`. |
| V5 | ✅ Resolved | `useAuth.ts` `startDeviceLogin` | `SLOW_DOWN` with `interval_seconds: 0` sets `intervalMs = 0`, so polling spins. The real provider never sends 0; this is defence in depth. **Resolved (green):** a grant or `SLOW_DOWN` without a positive interval ends the attempt in `failed` — a protocol error, not a spin. Pinned by two `DeviceLoginAcceptance.cy.tsx` tests. |
| V6 | 🟡 Low | `DaemonConfig.users` (`config.rs:341`) | Cloning a `DaemonConfig` now **shares** `users:`. Every current clone site wants that, but it is a semantic change to `Clone` that a future caller could trip over (a config cloned for a scratch edit shares the live rows). |
| V7 | ℹ Info | `DeviceLoginPanel.tsx:23` | The verification link uses `target="_blank"` inside the Tauri webview. It relies on `tauri-plugin-opener`'s link handling (`lib.rs:76`), as other dashboard links already do. Not exercised by any test, and still to confirm on the desktop. |
| V8 | ℹ Info | PRD § Technical Impact | OAuth App was chosen, but the device-flow response has not been checked against the live API for the absence of an expiring `refresh_token`. |

### V1 options (green, 2026-09-23)

> **Decided: option 2, transport-stamped metadata** (developer, 2026-09-23). Implemented as
> recorded under *Decisions during green*; the options below are kept as the record of what was
> weighed.

The fix needs a signal that says which transport a login arrived on, set by the transport and not
by the caller. **None exists today.** `tddy_rpc::RequestMetadata` carries only `sender_identity`,
which `ServerEngine::to_rpc_message` (`tddy-rpc/src/server_engine.rs:100`) copies from the request
envelope the sender writes itself. The Tauri IPC host, the LiveKit common room and the agent tool
socket all reach the same `Arc`'d `auth.AuthService` (`cloned_entries`). The open TODO at
`tddy-session-agents/src/service.rs:711-715` records the same gap. So V1 is not fixed here. The
options, for the developer:

1. **Per-transport rosters (recommended).** `runtime::build` registers `auth.AuthService` twice on
   an embedded host: once with `FirstLoginEnrolment` in `DaemonRuntime.entries`, which only the
   in-process IPC host serves, and once with no admission in the roster handed to the common room
   and the agent tool socket. It cannot be spoofed, because the choice is made at assembly and no
   request byte can affect it. Cost: the common-room roster is no longer `cloned_entries` of the
   in-process one, and a test must pin that the two differ only in that entry.
2. **Transport-stamped metadata.** Add `RequestMetadata.transport` (`InProcess` / `LiveKit` /
   `UnixSocket` / `Http`), set by each host where it builds the engine, overwriting anything from
   the envelope, and pass it to `LoginAdmission::admit`. It touches every transport in `tddy-rpc`,
   and `RequestMetadata::default()` would have to go, because any default value is exactly the
   hole this closes.
3. **Refuse enrolment while a common room is configured.** This is the smallest change, but it
   leaves the agent tool socket, a separate co-located process, able to enrol. It only works
   combined with (1) or (2) for that socket.

### Changeset sync

| Item | Was | Now |
|---|---|---|
| Draft PR contract | 🔲 | ✅ `9279fb17` |
| M1 seam + error returns | 🔲 | ✅ all 7 covered (`real_provider_over_http.rs`) |
| M2–M7 | 🔲 | ✅ |
| M8 | 🔲 | 🔲 pending the client id |
| Web flow signal "awaiting confirmation" | assumption | ✅ decided: `auth_flow` on both config paths |
| Decisions | 🔲 | ⚠ OAuth App plus rendered id decided; live-API check owed |
| Package documentation | 🔲 | 🔲 still none (`tddy-github/docs/device-flow.md` absent), left for wrap |
| Acceptance criteria | 0/10 | 9/10 (fresh-install criterion blocked on M8) |
| — | — | 🆕 `LoginAdmission` seam (`auth_service.rs:45`), `LiveUsers` (`live_users.rs`), `GitHubAuthFlow` / `auth_flow` on `/api/config` and `GetClientConfig`, `RealGitHubProvider::new_public*` |

## Acceptance Criteria

- [x] A daemon configured with **`client_id` only** registers `auth.AuthService` and serves `StartDeviceLogin`
- [x] `StartDeviceLogin` returns a `user_code` and `verification_uri`, and no client secret is sent
- [x] `PollDeviceLogin` returns `pending` until approval, then the same triple `ExchangeCode` returns
- [x] `slow_down`, `expired_token` and `access_denied` are honoured as distinct states
- [x] The **first** login on an empty `users:` is enrolled against the running OS user and persisted
- [x] A **second, different** login on an enrolled deployment is refused — no fallback
      (**decision, developer:** the login itself is minted and is **not** enrolled; every
      token-gated RPC it calls is refused `permission_denied`, as before — `first_login_admission.rs:63-70`)
- [x] `os_user_for_github` is unchanged and has no default arm
- [x] `client_id` + `client_secret` still serves the redirect flow unchanged
- [x] `RealGitHubProvider` takes a base URL; `exchange_code`'s six error returns are covered
- [ ] A fresh `./install --desktop` reaches a signed-in dashboard with **no file edited by hand** — 🔲 blocked on M8

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [x] M1 — the seam (do this first)
- [~] M2–M8 — M2–M7 done; M8 pending the client id
- [~] Answer the OAuth App / GitHub App question against the live API — **OAuth App** decided; live-API check still owed
- [x] Decide and record the client-id placement — rendered into `desktop.yaml.production`
- [ ] Package documentation for the six affected packages
- [ ] Package documentation for V1's API change, at wrap: `tddy-rpc` (the transport stamp and who
      sets it) and `tddy-stdio` (`from_duplex` now takes the transport its opener names — also
      worth a line where `packages/tddy-toolcall/docs/architecture.md` and
      `docs/ft/coder/rpc-multi-transport.md` describe `from_duplex`)
- [ ] `/wrap-context-docs` — deletes `2026-09-18-desktop-install-configures-no-identity.md` and the
      `missing-tests-real-exchange-code` record this node claims

# Changeset: Zero-configuration GitHub sign-in on Tddy Desktop

**Date**: 2026-09-19
**Status**: 🚧 In Progress
**Type**: Feature
**Stack**: `#keyring` 2/9 · branch `feature/keyring/desktop-login` · base `master` (parent `#keyring` 1/9 [#508](https://github.com/uppin/tddy-coder/pull/508) merged)

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
>
> **A second dev-dependency, at `/pr-wrap` step 2 (developer-approved, 2026-09-24):** `uuid` (v4) on
> `tddy-daemon-livekit`, **`[dev-dependencies]` only**, so `forwarded_rpc_is_stamped_by_the_receiver`
> names a common room of its own run on a reused LiveKit testkit container. Already in `Cargo.lock`
> through `tddy-testing-commons`; ships nothing.

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

Closed when a fresh `./install --desktop` reaches a signed-in dashboard with no file edited by hand
(deferred to the developer — 'I'll configure and test production myself'). This node's wrap deletes the entry.

### ⚠ DURING — models/agents open items at wrap — [`2026-08-16-models-agents-open-items-at-wrap.md`](../todo/2026-08-16-models-agents-open-items-at-wrap.md)

Touches this node's config surface. Recorded, not fixed.

### ⚠ DURING — Tauri desktop single-process daemon — [`2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md)

The application's own process **is** the daemon, which is why enrolment can know the running OS user
without asking anything. The entry's open items are unrelated to this node and stay open. This node extended the entry's comment-loss item with its own writer, `enrol_first_login` (V2) — recorded, not fixed.

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
- [~] **Implementation**: device flow, RPCs, the gate, enrolment, the web sign-in screen, transport-stamped metadata (V1) — done (M1–M7); `desktop.yaml.production` (M8) — deferred to the developer — 'I'll configure and test production myself'
- [~] **Decisions answered**: **OAuth App** (developer, during green) — ⚠ not yet verified against the live API; client id **rendered into `desktop.yaml.production`** — decided; the id itself and M8 are deferred to the developer — 'I'll configure and test production myself'
- [~] **Testing**: acceptance + unit written for every item, V1's transport tests included; the `tddy-web` spec passed 22/22 at `bba454da` — ⚠ not re-run by the 2026-09-24 validation; the orchestrator's scoped `cargo check --all-targets` over the 34 touched packages is the build gate for this run, and CI is the test gate (see *Validation Results*)
- [ ] **Package Documentation**: the six packages above
- [ ] **Code Quality**: scoped clippy per package; CI green — ⚠ not run by the 2026-09-24 validation (see *Validation Results*)

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
  - ⏸ **M8** — deferred to the developer — 'I'll configure and test production myself'. The file is unchanged at `HEAD` and still documents `client_id` +
    `client_secret` (`desktop.yaml.production:90-92`).

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
| `Http` | Connect-RPC `/rpc` router | `tddy-connectrpc/src/router.rs:181` (`over_http`) |
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
- [ ] **M8** — `desktop.yaml.production` and the `tddy-desktop` docs that state the barriers — ⏸ deferred to the developer — 'I'll configure and test production myself'

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

### From @validate-changes (2026-09-24)

- ~~**V9 — fix before merge:** resolve `TODO(#keyring 2/9)` at `tddy-coder/src/run.rs:1217`.~~ —
  done: `standalone_auth_provider` is the one decision the entry builder and `build_client_config`
  both read; `auth_flow` is `Some("redirect")` exactly when an entry is registered. Pinned by
  `run::standalone_auth_flow_declaration_tests`.
- ~~**V16 — fix before merge (docs):** add both green decisions to the PRD.~~ — done: § What's
  Changing and two ticked acceptance criteria citing their tests; the PRD checkboxes are synced.
- ~~**V10 — optional now, else record:** forward the received metadata in `tddy-bsp`.~~ — done in
  `bsp_service.rs` (`Request::with_metadata`). `daemon_rpc_handler.rs` stays `Direct` because it
  decodes a raw payload with no metadata in scope; `svc_split_context_from_codebase_host.rs` stays
  `Direct` because the daemon builds those requests itself. `RequestTransport::Direct`'s rustdoc is
  sharpened to match.
- ~~**V13 — optional:** drop `redirect_uri` from `RealGitHubProvider::new_public` /
  `new_public_with_base_urls`.~~ — done: dropped, and the secret and callback became one
  `Option<RedirectClient>`, so a public client holds no callback.
- Recorded only: V2 (comment stripping — add a `docs/dev/todo/` entry at wrap if still open), V6,
  V7, V8, V11, V12, V14, V15, V17.

### From /validate-tests

**Fix before merge (test-only):**
- ~~**T1 (critical):** Cypress test for a COMPLETE poll missing user / session token / refresh token → attempt `failed`, signed out, nothing stored.~~ ✅ Done (step 2): `aCompletedPollMissing(part)` in `deviceLoginBackend.ts`; `DeviceLoginAcceptance.cy.tsx` "ends the attempt as failed when an approval arrives without a whole session", once per missing part (3 tests), asserting the exact message, `expectSignedOut()` and nothing under `ACCESS_TOKEN_KEY`.
- ~~**T2 (critical):** `real_provider_over_http.rs` — the fake records path+query, headers and body; the no-secret test polls through to `Complete` and asserts no part of any request carries the secret; assert the endpoint sequence in `a_public_client_signs_in_by_the_device_flow`.~~ ✅ Done (step 2): `AReceivedRequest { path_and_query, headers, body }`; the no-secret test scripts DeviceCode → AccessToken → User, polls to `Complete`, and asserts the three endpoints were hit and no part of any request holds the secret; the public-client test asserts `/login/device/code`, `/login/oauth/access_token`, `/user` in order.
- ~~**T3:** exact error prefixes instead of `is_err()` for the interval-less `slow_down` tests.~~ ✅ Done (step 2): `assert_refused_for_want_of_an_open_attempt` asserts the `real.rs` `widened_interval` prefix ("GitHub asked to slow down polling for a device code with no open attempt on this daemon") at all three sites.
- ~~**T4:** consecutive `slow_down` widening (5→10→15), and an unknown device error that fails and forgets the code.~~ ✅ Done (step 2): `a_second_slow_down_widens_from_the_first`, `an_unknown_device_error_fails_and_forgets_the_code` (`incorrect_device_code` → `Err("device login failed: incorrect_device_code")`, then an interval-less `slow_down` is refused for want of an open attempt).
- ~~**T5:** bound the poll loops in `tddy-github/src/auth_service.rs` tests and `tddy-daemon/tests/first_login_enrolment_acceptance.rs`.~~ ✅ Done (step 2): both loops are `for _ in 0..POLLS_BEFORE_GIVING_UP` (a local constant of 5, documented as exceeding the stub's pending polls) and then `panic!` naming the states seen. The `STUB_DEVICE_LOGIN_*` constants are **not** needed outside `stub.rs`, so the prod-ready finding may make them private.
- ~~**T6:** `auth_service.rs` — a scripted provider pins `SlowDown{interval}` / `Denied` / `Expired` → proto state and interval.~~ ✅ Done (step 2): `ScriptedDeviceProvider` plus three tests asserting `(state(), interval_seconds)`: `(SlowDown, 10)`, `(Denied, 0)`, `(Expired, 0)`.
- ~~**T7:** `FirstLoginEnrolment` unit tests — every non-`InProcess` transport admits unmapped and writes nothing; `InProcess` enrols; an unwritable file → `failed_precondition`.~~ ✅ Done (step 2): `#[cfg(test)] mod tests` in `first_login_admission.rs`. All six non-`InProcess` transports → `Ok(())`, an empty snapshot and a byte-identical file. `InProcess` → enrolled in memory and on reload. A config file that has gone (the kernel's own `ConfigNotWritable` precedent) → `Code::FailedPrecondition` with the exact reason prefix, nobody mapped.
- ~~**T8:** bidi continuation frame claiming `InProcess` is stamped by the host (`server_engine_stamps_transport.rs`).~~ ✅ Done (step 2): `a_bidi_continuation_claiming_the_in_process_bridge_is_stamped_by_the_host`. It opens honestly with `end_of_stream: false`, sends a continuation claiming `InProcess`, and asserts the session, the opening message and the continuation are all `LiveKit`.
- ~~**T9:** `forwarded_rpc_is_stamped_by_the_receiver.rs` — UUID room name; abort the peer task on drop.~~ ✅ Done (step 2): the room is `forwarded-rpc-stamp-<uuid v4>`, and the peer task is held in an `AbortedOnDrop` guard. `uuid` was added as a **dev-dependency** of `tddy-daemon-livekit`. It is already in `Cargo.lock` (1.23.3) through that crate's `tddy-testing-commons` dev-dep, so it is a new edge, not a new crate.
- ~~**T10:** `expectFailedMessage(containing)` with the specific text in the two interval-less tests.~~ ✅ Done (step 2): the two interval-less tests pass the exact `useAuth.ts` strings.
- ~~**T11:** `device_login_acceptance.rs` — reword the When; prove the approved token authenticates; exact stub values.~~ ✅ Done (step 2): the Given and When now say the stub approves on the poll after the first; `GetAuthStatus` with the approved token → `(true, Some("operator"))`; the start asserts `STUB-0001`, `https://github.com/login/device`, `stub-device-code-1`, 900, 1.
- ~~**T12:** reword the "daemon predating the device flow" Then-comments (`server_options_acceptance.rs`, `daemon_config_service.rs`).~~ ✅ Done (step 2): both now read "absent means this daemon serves no GitHub sign-in".
- ~~**T13:** justify `SERVING_TIMEOUT` (20 s) and `PARTICIPANT_TIMEOUT` (10 s); extract `expectLatestPollPresented`; move the trailing `use` in `real_provider_over_http.rs`.~~ ✅ Done (step 2): both timeouts carry a justification comment (LiveKit in Docker on a loaded CI runner); `expectLatestPollPresented` is extracted; the `use` is in the top import block.

**Recorded only:**
- Remaining `real.rs` device-flow error branches (start/poll non-2xx and unparseable, `(None, None)`, a user-leg failure after a device grant, a remembered explicit interval).
- Second-account test uses the redirect flow (stub completes device logins only as the first user); both flows share `complete_login`.
- `runtime::build` refusal boundaries (non-empty `users:`, no `github:`) unpinned; loose `contains`.
- Kernel `enrolment_keeps_everything_else_the_config_says` compares two fields only.
- Web: `StartDeviceLogin`/`PollDeviceLogin` RPC failure and unrecognised `DeviceLoginState` untested.
- Generated bidi handler metadata, and the `Http` (connectrpc router) / `Grpc` (tonic adapters) stamps, untested — none can enrol today.
- Free-port bind-then-drop races (repo pattern); `DeviceLoginAcceptance.cy.tsx` `/api/config` cases duplicate `clientConfig.test.ts`; `sees_participant` duplicated across two LiveKit suites.

### From /validate-prod-ready

- ~~**Fix now:** `tddy-github/src/stub.rs:11,15` — `STUB_DEVICE_LOGIN_PENDING_POLLS` / `STUB_DEVICE_LOGIN_INTERVAL_SECONDS` to private `const`; nothing outside `stub.rs` reads them.~~ **Done in pr-wrap step 3** — both private; re-grepped `packages/`, no reader outside `stub.rs`, no `lib.rs` re-export; the struct doc names the constant as plain code instead of an intra-doc link to a private item.
- ~~**Fix now (V2 debt):** extend `docs/dev/todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md:11` to name `tddy-daemon-kernel/src/first_login_enrolment.rs` `enrol_first_login` as the second writer that loses comments — on every desktop's first sign-in, header included — and point the TODO at `first_login_enrolment.rs:84` to that entry.~~ **Done in pr-wrap step 3** — the backlog bullet now names both writers and one fix; the TODO is `TODO(docs/dev/todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md)`. The comment loss itself is recorded, not fixed.
- Recorded only: `first_login_enrolment.rs:61` `pub fn enrol_first_login` skips `LiveUsers`' file-write lock (production caller is `live_users.rs` only; narrowing it means moving `tddy-daemon-kernel/tests/first_login_enrolment_acceptance.rs` onto `LiveUsers::enrol_first_login`); `live_users.rs` `snapshot` and `From<Vec<UserMapping>>` are test-support API.

## Validation Results

**Run:** `/validate-changes`, 2026-09-24, `pr-509-green` @ `09ca3eb3`, base `origin/master`. Supersedes
the 2026-09-23 run at `bba454da`; the two commits since (`55a44090` no fallback survives the
device-flow sign-in, `09ca3eb3` V1 transport-stamped metadata) were read in full, and V2–V8
re-checked against the current code.

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes, planned. #508 (`#keyring` 1/9) **merged**; the PR base is now `master` |
| `/pr-stack-rebase` | ✅ Already current: `git merge-base --is-ancestor origin/master HEAD` holds; no rebase run (gate passed by the orchestrator) |
| Leak check (`origin/master..HEAD`) | ✅ Clean: exactly this PR's 10 commits, `c11d4a14` … `09ca3eb3`, all `(#keyring 2/9)` |
| Diff contains only this PR's files | ✅ 253 files. ~190 of them are V1's mechanical ripple (~590 `Request::new` → `Request::direct`, ~90 metadata sites, `from_duplex` / `ServerEngine::new` / `start_bidi_stream` signatures), skimmed rather than read one by one. The rest are claimed by an item here |
| Parent-owned files intact | ✅ No deletions (`--diff-filter=D` is empty) |

### Stack boundary

| Check | Result |
|---|---|
| Changeset items implemented or deferred | ⚠ 1 open, deferred: M8 (`desktop.yaml.production`) and with it the fresh-install criterion — deferred to the developer — 'I'll configure and test production myself' |
| `## Responsibility` delivered | ✅ No stubs. Two TODOs remain: `first_login_enrolment.rs:84` (V2, recorded) and `tddy-coder/src/run.rs:1217` — tagged `TODO(#keyring 2/9)`, i.e. **this** node (V9) |
| `## Dependencies` not implemented here | ✅ Clean. Consumes #508's `SessionTokens`, `build_auth_entries_with` and the `v2` signer without editing them |
| `## Boundaries` respected | ✅ No second-account path; `os_user_for_github` has no default arm; `auth.rs` / `config.rs` not split. No token-store change (3/9) — `token_store.rs` untouched, the device flow retains through the existing `GitHubTokenStore`. No link-github surface (8/9); the `telegram_github_link.rs` edits are the `authorize_url -> Result` ripple only |
| No dependent's behaviour | ✅ Clean |
| New dependencies | ✅ `axum 0.8` on `tddy-github`, `[dev-dependencies]` only, as recorded; `Cargo.lock` gains that one edge |

### Build and tests (scoped)

This run did **not** run cargo: the orchestrator runs a scoped `cargo check --all-targets` over the 34
touched packages, and CI is the whole-workspace gate.

| Package | Build | Tests |
|---|---|---|
| the 34 touched Rust packages | see orchestrator | not run by this validation — CI |
| tddy-web `DeviceLoginAcceptance.cy.tsx` | n/a | ✅ 22/22 at `bba454da`; ⚠ `55a44090` changed the spec and `clientConfig.test.ts` — not re-run here |
| `/pr-wrap` step 1 refactor (V9, V10, V13): tddy-rpc, tddy-github, tddy-bsp, tddy-daemon-auth, tddy-coder | ✅ `cargo check --all-targets` and `cargo clippy --all-targets -- -D warnings` clean over all five | ✅ `./test -p tddy-rpc -p tddy-github -p tddy-bsp -p tddy-daemon-auth`: 237 passed, 0 failed (30 binaries); ✅ `./test -p tddy-coder --lib`: 107 passed, 0 failed (includes the 5 new `standalone_auth_flow_declaration_tests`); tddy-coder integration suites not run locally, so CI covers them |
| `/pr-wrap` step 2 refactor (T1–T13, test-only): tddy-github, tddy-daemon-auth, tddy-rpc, tddy-daemon-livekit, tddy-daemon, tddy-web | ✅ `cargo clippy -p <pkg> --all-targets -- -D warnings` clean on all five Rust packages | ✅ `./test -p tddy-github -p tddy-daemon-auth`: 195 passed, 0 failed; ✅ `./test -p tddy-rpc`: 39 passed, 0 failed; ✅ `./test -p tddy-daemon-livekit --test forwarded_rpc_is_stamped_by_the_receiver`: 1 passed; ✅ `./test -p tddy-daemon --test first_login_enrolment_acceptance --test server_options_acceptance --test daemon_config_service`: 9 + 14 + 20 passed, 0 failed; ✅ `DeviceLoginAcceptance.cy.tsx` alone: 28/28 (25 before, plus the 3 T1 cases). Other tddy-daemon suites are left to CI |

### Fallback scan (production code added since `origin/master`)

**No fallback introduced.** Every `unwrap_or*` / `Default` / silent `Ok` among the added production
lines, and why it is not one:

| Where | What | Verdict |
|---|---|---|
| `tddy-daemon-auth/src/auth.rs:89` `github.stub.unwrap_or(false)`, `:213` redirect-URI default, `:217` `"stub-client-id"` | moved from the old `if/else if` gate into `github_provider_kind` / the `match` | pre-existing, not introduced. `:213` now also feeds the **public** provider, which never uses it (V13) |
| `tddy-github/src/real.rs:197` `name.unwrap_or_default()` | GitHub's nullable display name | pre-existing (`origin/master` `real.rs:120`), moved into `fetch_user` |
| `tddy-github/src/real.rs:409` `.unwrap_or_default()` | formats an absent `error_description` as nothing inside an `Err` | not a fallback |
| `tddy-daemon-kernel/src/live_users.rs:23,28` `#[derive(Default)]` | empty `users:` — refuses everyone | the safe direction, and exactly the old `#[serde(default)] Vec` |
| `tddy-daemon-auth/src/first_login_admission.rs:53,70,85,95` `Ok(())` | mapped login; non-`InProcess` transport; `AlreadyEnrolled` | deliberate: the login is minted *unmapped* and every token-gated RPC refuses it — the developer's decision, not an admission |
| `tddy-rpc/src/server_engine.rs:256,361` `let _ = …send(…)` | pre-existing shape, now with `self.to_rpc_message` | unchanged semantics |
| `tddy-web/src/hooks/useAuth.ts:257` `res.user ?? null` | `ExchangeCode` without a user → signed in with `user: null` | pre-existing behaviour, moved from inside `adoptSession`; the device path now refuses the same shape (`COMPLETE` without a user → `failed`), so the two flows differ here (V14) |

Removed by `55a44090`: the 5 s `DEFAULT_DEVICE_POLL_INTERVAL_SECONDS`; absent `auth_flow` read as the
redirect flow; unknown `auth_flow` read as absent; `GetAuthUrl` handing a public client a URL; an
embedded host with no config path silently never enrolling. `09ca3eb3` removed
`RequestMetadata: Default` and `Request::new`.

No `println!` / `eprintln!` added. New `unwrap`/`expect` in production are lock-poisoning only
(`real.rs:203`, `live_users.rs:53,102,112,124`, `stub.rs`).

### Risks

| # | Severity | Where | Finding |
|---|---|---|---|
| V1 | ✅ Resolved (was 🔴 High, security) | `tddy-rpc/src/message.rs`, `server_engine.rs` `metadata_of`, `first_login_admission.rs:57-72` | Re-checked at `09ca3eb3`. Every `ServerEngine::new` names its transport (5 sites: `tauri-rpc/host.rs:95`, `multi_host.rs:120` `InProcess`; `livekit/participant.rs:338,495` `LiveKit`; `stdio/endpoint.rs:65` from its opener); the Connect router stamps `Http` (`router.rs:181`); tonic stamps `Grpc`. No production code reads the envelope for the transport, and `RequestTransport::InProcess` is constructed only by the two Tauri hosts. The admission match is exhaustive. Both Tauri hosts are pinned (`tddy-tauri-rpc/tests/stamps_the_in_process_transport.rs`); the acceptance suite drives the roster with a hand-stamped `InProcess` message, plus the real `LiveKitParticipant` and agent tool socket for the refusals. Residual notes: V10, V11, V12 |
| V2 | 🟠 Medium, open | `tddy-daemon-kernel/src/first_login_enrolment.rs:84` | Unchanged: the first login rewrites `~/.tddy/desktop.yaml` through `serde_yaml::Value` and strips every comment, the rendered explanatory header included. Hits every desktop on its first sign-in. The TODO now references the backlog entry [`2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`](../todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md), which names this writer beside `daemon_config_service.rs` (pr-wrap step 3) |
| V3 | ⏸ Deferred | `desktop.yaml.production:90-92` | M8 — deferred to the developer — 'I'll configure and test production myself'. The file is unchanged and still documents `client_secret`. Not a gap of this run |
| V4 | ✅ Resolved | `real.rs` `device_attempts` | Re-checked: `DeviceAttempt { interval_seconds, expires_at }`, pruned on every start and poll, removed on every terminal answer |
| V5 | ✅ Resolved | `useAuth.ts:311,327` | Re-checked: a non-positive grant or `SLOW_DOWN` interval ends the attempt `failed` |
| V6 | 🟡 Low, open | `tddy-daemon-kernel/src/config.rs:341` | Unchanged: cloning a `DaemonConfig` shares `users:` (`LiveUsers`). Documented at the field and in `live_users.rs` |
| V7 | ℹ Info, open | `DeviceLoginPanel.tsx:23` | Unchanged: `target="_blank"` inside the Tauri webview, relying on `tauri-plugin-opener`. Untested; confirm on the desktop |
| V8 | ℹ Info, open | PRD § Technical Impact | Unchanged: the OAuth App's device-flow response not yet checked against the live API for the absence of an expiring `refresh_token`. Part of the developer's production check |
| V9 | ✅ Resolved (was 🟠 Medium) | `tddy-coder/src/run.rs` `standalone_auth_provider` | Refactored in `/pr-wrap` step 1: `StandaloneAuthProvider { Stub, Confidential }` + `standalone_auth_provider(args)` is the one decision both `build_auth_service_entry` (now an exhaustive `match`) and `build_client_config` read. `auth_flow` is `Some("redirect")` exactly when an entry is registered, `None` otherwise; the TODO is gone. The standalone server has no public/device provider (a client id without a secret registers nothing), so it never declares `"device"`. Pinned by `run::standalone_auth_flow_declaration_tests` (5 cases: stub, stub codes alone, confidential, id without secret, no GitHub args) |
| V10 | ✅ Resolved where the metadata is in scope (was 🟡 Low) | `tddy-bsp/src/bsp_service.rs`; `tddy-rpc/src/message.rs` `RequestTransport::Direct`, `types.rs` `Request::direct` | `DaemonBspService`'s seven relays now capture `request.metadata().clone()` before `into_inner()` and rebuild with `Request::with_metadata(req, metadata)`, so the inner `BspServiceImpl` sees the stamp its host gave the request. **Left as `Direct`, by design:** `tddy-session-lifecycle/src/connection_service/daemon_rpc_handler.rs` (`HostRpcHandler::handle_rpc(service, method, payload: &[u8])` receives raw bytes, with no metadata in scope to pass on) and `svc_split_context_from_codebase_host.rs:216,235` (requests the daemon builds itself from its own state, not relays). `Direct`'s rustdoc now says it is a call made in code carrying no transport claim, including a payload re-dispatched after its request was decoded, and that a relay that still holds the original metadata must pass it through. `Direct` is never a stand-in for an unknown transport |
| V11 | ℹ Info, **new** | `first_login_admission.rs`, `auth_service.rs:331` | Enrolment is decided by the transport of the *completing* poll only; the `StartDeviceLogin` that issued the code is not bound to it. Not exploitable — a `device_code` is returned only to its starter — but binding the attempt to its starting transport would be defence in depth |
| V12 | ℹ Info, **new** | `tddy-desktop/src-tauri/tauri.conf.json` `"csp": null` (pre-existing) | `InProcess` now means "the person at the machine", so script running in the dashboard webview can enrol. Capabilities are local-only (no `remote`), so only an XSS in the pre-sign-in dashboard could exploit it. Pre-existing; outside this PR |
| V13 | ✅ Resolved (was ℹ Info) | `tddy-github/src/real.rs`; `tddy-daemon-auth/src/auth.rs:236`; `tddy-github/tests/real_provider_over_http.rs` | `new_public(client_id)` / `new_public_with_base_urls(client_id, oauth, api)` take no redirect URI. `client_secret: Option<String>` + `redirect_uri: String` became one `redirect_client: Option<RedirectClient { client_secret, redirect_uri }>`, so a public client holds no callback at all, rather than a placeholder. `authorize_url` and `exchange_code` refuse on `None` exactly as before. The defaulted `redirect_uri()` in `build_auth_entries_admitting` now feeds only the stub and the confidential client |
| V14 | ℹ Info, **new** | `useAuth.ts:257` vs `:339` | `ExchangeCode` without a `user` still signs in with `user: null`; `COMPLETE` without one fails. Pre-existing on the redirect side; the two flows now disagree about the same shape |
| V15 | ℹ Info, **new** | `tddy-web/src/index.tsx:133`, `rpc/clientConfig.ts:140` | Pre-existing, documented: an unreachable daemon or a non-OK `/api/config` renders the standalone connection form (`daemonMode: false`). On a desktop a failed `GetClientConfig` would show that form rather than an error. Not introduced here |
| V16 | ✅ Resolved (was ℹ Info) | PRD `docs/ft/desktop/1-WIP/PRD-2026-09-19-keyring-desktop-login.md` | Both green decisions are now in § What's Changing ("Only a login completed from the desktop's own window enrols"; "The sign-in flow is declared, never inferred") and are ticked acceptance criteria citing their tests. The PRD's checkboxes are synced with this changeset: 11 of 12 ticked, and `./install --desktop` stays unticked and deferred to the developer |
| V17 | ℹ Info, carried | `first_login_admission.rs` / `live_users.rs` `enrol_first_login` | Synchronous file I/O under a `std::sync::Mutex` on an async RPC task, once per deployment. Acceptable; `spawn_blocking` would be tidier |

### /validate-tests (2026-09-24)

**Run:** `/validate-tests`, 2026-09-24, `pr-509-green` @ `09ca3eb3`, base `origin/master`.

- **Tests analyzed:** 85 added/meaningfully changed (Rust 56, Cypress 25, vitest 4) across 14 files, plus a scan of ~590 `Request::new` → `Request::direct` substitutions.
- **Status:** ⚠ 2 critical, 22 warnings → the fix-now set is addressed in the step 2 refactor (see *From /validate-tests*).
- **Substitutions:** mechanical (598 `Request::new(` removed, 592 `Request::direct(` added, ~98 `RequestMetadata::over(Direct)`, `A_PIPE` in dispatch-only engine suites). No test proves the wrong thing through its transport: every transport-sensitive test stamps `InProcess` / `LiveKit` / `UnixSocket` explicitly or goes through the real host.
- **Markers:** no `#[ignore]`, `.only`, `.skip` or `sleep`; Cypress timing is `cy.clock`-driven.
- **No-fallback decisions pinned:** absent `auth_flow` ✅, unknown `auth_flow` ✅, public client refuses `GetAuthUrl` ✅, slow_down without interval ✅ (provider assertions loose — T3), `runtime::build` refusal ✅ (positive case only), COMPLETE missing user/tokens ❌ unpinned (T1).
- **Critical:** `DeviceLoginAcceptance.cy.tsx` had no test for a COMPLETE poll missing its user or tokens (`useAuth.ts`); `real_provider_over_http.rs` `no_request_in_the_device_flow_carries_the_client_secret` recorded bodies only and never polled to `Complete`.
- **Description-body mismatches:** `device_login_acceptance.rs` ("When they approve it" — the stub approves); `server_options_acceptance.rs` and `daemon_config_service.rs` framed an absent `auth_flow` as backward compatibility; kernel `first_login_enrolment_acceptance.rs` says "only `users:` changed" but compares two fields.

### /validate-prod-ready (2026-09-24)

**Run:** `/validate-prod-ready`, 2026-09-24, `pr-509-green` @ `09ca3eb3`, base `origin/master`, over 78 production files (2,187 added lines); 175 excluded (tests, cypress, `src/gen/`, docs, `Cargo.lock`).
**Status:** ⚠️ Gaps → fixed in the step 3 refactor (see below) — 0 blockers.

| Category | Count | Status |
|---|---|---|
| Mock code | 0 | ✅ `StubGitHubProvider`'s device-flow methods (`stub.rs`) follow the existing production-shipped stub: no `cfg(test)`/env branch, deterministic, fail loudly with no `stub_codes`. Reachable only by direct RPC — a stub daemon declares `redirect` |
| Dev fallbacks | 0 new | ✅ Every `unwrap_or*`/`Default` among the added lines is moved from master (`auth.rs` stub flag, redirect-URI default, `"stub-client-id"`; `real.rs` `name.unwrap_or_default()`), error formatting only, or already in the fallback table. `runtime.rs` `cfg(not(unix)) this_process_os_user() → None` fails `runtime::build` loudly. No env-conditional branch |
| TODO/FIXME | 2 | ⚠️ `run.rs` `TODO(#keyring 2/9)` — V9, fixed in step 1. `first_login_enrolment.rs:84` — V2, unreferenced → referenced to the backlog entry in step 3 |
| Unused code | 3 | ⚠️ `stub.rs` `pub const STUB_DEVICE_LOGIN_*` referenced only in `stub.rs`; `live_users.rs` `snapshot` / `From<Vec<UserMapping>>` test-only support API; `first_login_enrolment.rs` `pub fn enrol_first_login` bypasses `LiveUsers`' file lock (one kernel acceptance test uses it). `new_public*`'s `redirect_uri` — V13, step 1 |
| Debug output | 0 | ✅ No `println!`/`eprintln!`/`dbg!`/`console.*` added |

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

| Item | Was (2026-09-23) | Now (2026-09-24) |
|---|---|---|
| Header `**Stack**:` | base `feature/keyring/signing-key` (#508) | base `master`; #508 merged |
| M1–M7 | ✅ | ✅ (unchanged) |
| V1 transport-stamped metadata | 🔴 open risk | ✅ implemented (`09ca3eb3`); 🆕 `RequestTransport`, `RequestMetadata::over`, `Request::direct`, `ServerEngine::new(service, transport)`, `start_bidi_stream(.., metadata, ..)` |
| "No fallbacks" decisions | — | ✅ implemented (`55a44090`); 🆕 `AuthFlowDeclaration` (`"none"` / `{ unrecognised }`), `authorize_url -> Result`, `DeviceAttempt`, `runtime::build` refusal |
| M8 | 🔲 pending the client id | ⏸ deferred to the developer — 'I'll configure and test production myself' |
| Testing | ⚠ blocked by a full disk | ⚠ not run by this validation; orchestrator's scoped `cargo check` + CI |
| Code quality | ⚠ blocked by a full disk | ⚠ scoped clippy not run by this validation |
| Package documentation | 🔲 | 🔲 still none (`tddy-github/docs/device-flow.md` absent; `tddy-rpc` / `tddy-stdio` transport docs owed), left for wrap |
| Acceptance criteria | 9/10 | 9/10 — the fresh-install criterion is deferred to the developer — 'I'll configure and test production myself' |
| Stamping-site table | — | corrected `router.rs:182` → `:181` |

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
- [x] Only a login completed over the desktop's own window (`InProcess`) enrols; any other transport
      on an unenrolled desktop is minted unmapped and writes nothing (`first_login_enrolment_acceptance.rs`)
- [x] An absent `auth_flow` means "no sign-in configured", an unknown value is an error, and a public
      client refuses `GetAuthUrl` with `failed_precondition` (`device_login_acceptance.rs`,
      `clientConfig.test.ts`, `DeviceLoginAcceptance.cy.tsx`)
- [ ] A fresh `./install --desktop` reaches a signed-in dashboard with **no file edited by hand** — ⏸ deferred to the developer — 'I'll configure and test production myself'

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2
- [x] M1 — the seam (do this first)
- [~] M2–M8 — M2–M7 done; M8 deferred to the developer — 'I'll configure and test production myself'
- [~] Answer the OAuth App / GitHub App question against the live API — **OAuth App** decided; live-API check still owed
- [x] Decide and record the client-id placement — rendered into `desktop.yaml.production`
- [ ] Package documentation for the six affected packages
- [ ] Package documentation for V1's API change, at wrap: `tddy-rpc` (the transport stamp and who
      sets it) and `tddy-stdio` (`from_duplex` now takes the transport its opener names — also
      worth a line where `packages/tddy-toolcall/docs/architecture.md` and
      `docs/ft/coder/rpc-multi-transport.md` describe `from_duplex`)
- [ ] `/wrap-context-docs` — deletes `2026-09-18-desktop-install-configures-no-identity.md` and the
      `missing-tests-real-exchange-code` record this node claims

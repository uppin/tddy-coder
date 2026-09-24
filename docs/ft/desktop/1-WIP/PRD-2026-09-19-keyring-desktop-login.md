# Zero-configuration GitHub sign-in on Tddy Desktop - PRD

**Date**: 2026-09-19
**PRD Type**: Feature
**Stack**: `#keyring` 2/9 — depends on 1/9 `signing-key`

## Affected Features

- **Primary**: [Tddy Desktop](../tddy-desktop-tauri.md) — the document records three barriers that
  stop a fresh `./install --desktop` from signing in. `#keyring` 1/9 removed the second; this node
  removes the **first and third**, so the section becomes a description of a working first run
  rather than a list of what an operator must hand-edit.
- **Primary**: [Cross-daemon session authentication](../../daemon/session-auth.md) — a second way to
  establish a session appears beside the redirect flow. The token that results is identical; only
  how GitHub was convinced differs.
- **Related**: [Daemon settings](../../daemon/daemon-settings.md) — the settings screen stops being
  the thing a user cannot reach because they cannot sign in.

## Summary

A user downloads Tddy Desktop, opens it, and signs in to GitHub. Nothing is hand-edited, no
`client_secret` is embedded anywhere, and no `users:` mapping is written by hand.

Two changes make that true. The daemon gains the **GitHub OAuth device flow** — the same flow the
GitHub CLI uses — which authenticates with a **public client id alone** and never needs a client
secret, so barrier 1 disappears. And a desktop deployment **enrols its first successful login**: the
GitHub account that signs in first is bound, once and explicitly, to the OS user the application
already runs as, and that binding is persisted. Barrier 3 disappears without a fallback being
introduced anywhere.

## Background

`desktop.yaml.production:66-92` names the three barriers itself, and each is a distinct code site:

| # | Barrier | Site | Status |
|---|---|---|---|
| 1 | `github:` needs **both** `client_id` and `client_secret`, or no auth entry is registered | `packages/tddy-daemon-auth/src/auth.rs:109` — `else if let (Some(id), Some(secret)) = (…)` | **this node** |
| 2 | `livekit.api_secret` is the only source of the session-token signer | `auth.rs:68` | closed by `#keyring` 1/9 |
| 3 | `users: []` — a login with no entry is refused `permission_denied: user not mapped to OS user` | `packages/tddy-daemon-kernel/src/config.rs:1111` `os_user_for_github` | **this node** |

`./install --desktop` renders all three unset and never overwrites an existing `~/.tddy/desktop.yaml`,
so the installed application opens on its settings screen with no sessions. Observed in
`~/.tddy/logs/daemon`:

```
MultiRpcService.handle_rpc: NO service 'auth.AuthService'
  (registered: ["daemon_config.DaemonConfigService", …])
```

`auth.AuthService` is never registered at all, which is why the dashboard's `GetAuthUrl` answers
`not_found` rather than an error a user could act on.

**There is no device-flow code anywhere in the tree.** `packages/tddy-github/src/real.rs` holds one
OAuth path: `authorize_url` at `:45` and the redirect-flow `exchange_code` at `:59`, which posts
`client_secret` to a hardcoded `https://github.com/login/oauth/access_token` and then fetches
`https://api.github.com/user`.

## Proposed Changes

### What's Changing

**A second provider mode: the device flow.** `GitHubOAuthProvider`
(`packages/tddy-github/src/provider.rs:16`) has three methods today and none of them fits a flow
with no redirect and no authorization code. The trait gains a device-flow pair:

- `start_device_login()` → `{ device_code, user_code, verification_uri, expires_in, interval }`;
- `poll_device_login(device_code)` → pending / slow-down / denied / expired / `(access_token, GitHubUser)`.

GitHub's device endpoint takes `client_id` and `scope` and **no client secret**, which is the whole
reason this removes barrier 1.

**Two new RPCs on `auth.AuthService`** (`packages/tddy-service/proto/auth.proto`), mirroring the
existing `GetAuthUrl` / `ExchangeCode` pair:

```proto
rpc StartDeviceLogin(StartDeviceLoginRequest) returns (StartDeviceLoginResponse);
rpc PollDeviceLogin(PollDeviceLoginRequest) returns (PollDeviceLoginResponse);
```

`PollDeviceLogin` returns the same `session_token` + `refresh_token` + `GitHubUser` triple
`ExchangeCode` does. **The session that results is the same session**: `#keyring` 1/9's `v2` token,
the same 5-minute/7-day sliding TTLs, the same `kind` enforcement.

**`build_auth_entries` stops requiring a client secret.** The gate at `auth.rs:109` becomes: a
`client_id` alone registers a **device-flow provider**; `client_id` + `client_secret` continues to
register the redirect-flow provider, unchanged, because that is what a served deployment behind a
real callback URL wants. Neither is a fallback for the other — they are two configurations, and a
deployment declares which one it is by what it supplies.

**First-login enrolment replaces the empty `users:` list, explicitly.** On a desktop deployment, the
first successful login writes a `users:` entry binding that GitHub login to the OS user the
application is already running as, persisted to `~/.tddy/desktop.yaml`. Thereafter that entry is
matched by the ordinary `os_user_for_github` lookup, and **a different GitHub login is refused
exactly as it is today**.

> **This is enrolment, not a fallback, and the distinction is the design.** A fallback would answer
> "this login has no mapping" by inventing one *every time* — which is precisely the unsafe shape
> CLAUDE.md forbids, and it would let any GitHub account on earth drive the desktop user's machine.
> Enrolment answers it **once**, on a deployment that has never had an identity, and the answer is
> then a configured fact that behaves like a hand-written one. `os_user_for_github` is not changed
> and gains no default arm; the enrolment writes the row the lookup then finds.

**Only a login completed from the desktop's own window enrols** (decided during green). Whether a
completing login is "the person at the machine" is decided by the transport the call arrived on, as
stamped by the host that received it (`tddy_rpc::RequestTransport`), never by anything the caller
wrote. Only `InProcess` (the desktop's Tauri IPC bridge) enrols. A login completed over any other
transport on an unenrolled desktop (the LiveKit common room, a session room, the agent tool socket)
is minted **unmapped** and writes nothing. Every token-gated RPC it calls is refused
`permission_denied`, and the desktop can still enrol its own window's login afterwards. A server
deployment has no admission at all and is unaffected.

**The sign-in flow is declared, never inferred** (decided during green). A daemon declares its flow
in `auth_flow`, at `GET /api/config` and in `GetClientConfig`: `"redirect"` or `"device"`, exactly
the provider it registered. An **absent** `auth_flow` means "this daemon has no GitHub sign-in
configured", and the dashboard offers neither flow. It does not offer the redirect button. An
**unknown** value is an error, not a guess. A public client (`client_id` with no secret) refuses
`GetAuthUrl` with `failed_precondition`, naming the device flow, just as it refuses `ExchangeCode`.
There is no compatibility path for an older daemon or dashboard, because every desktop is rolled out
together.

**The base-URL seam in `RealGitHubProvider`, opened before the device flow is written.**
[`missing-tests-real-exchange-code`](../../../../packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md)
records that `exchange_code` is untestable by construction: two hardcoded absolute hosts, no
base-URL field on the struct (`real.rs:11`) or its constructor (`:33`), and 0 of its 6 error returns
exercised. This node adds ~100 more lines of network code plus a **polling state machine** to that
same provider. The seam is opened first, or it gets written twice and the harder half stays
untested. This node claims that record.

### What's Staying the Same

- **The redirect flow**, for served deployments. `client_id` + `client_secret` behaves exactly as it
  does today, including `redirect_uri` defaulting to `http://{web_host}:{web_port}/auth/callback`.
- **`StubGitHubProvider`** and `github.stub_codes`, which the acceptance suites depend on. The stub
  gains the two device-flow methods; its synthetic token still reports
  `issues_usable_access_token() == false`.
- **`os_user_for_github`** — same linear search, same `None` for an unmapped login, same
  `permission_denied`. The lookup is untouched.
- **Token retention.** The GitHub access token is still kept out of the session token handed to the
  browser, per `token_store.rs`'s standing rule. Where it is *stored* changes in `#keyring` 3/9, not
  here.
- **`./install --desktop`'s contract** — `~/.tddy/desktop.yaml` is still never overwritten, and it is
  still the only file a release build reads.

## Impact Analysis

### Technical Impact

| Package | Change |
|---|---|
| `tddy-github` | `provider.rs` — two trait methods; `real.rs` — the base-URL seam **first**, then the device flow and its polling state machine; `stub.rs` — stub implementations |
| `tddy-service` | `auth.proto` — `StartDeviceLogin` / `PollDeviceLogin` and their messages |
| `tddy-daemon-auth` | `auth.rs:109` — the gate; `auth_service.rs` — the two handlers; first-login enrolment |
| `tddy-daemon-kernel` | `config.rs` — persisting the enrolled `users:` entry. **The file is not split** |
| `tddy-desktop` | `desktop.yaml.production` — a public `client_id` present by default, `client_secret` absent |
| `tddy-web` | the sign-in screen: show `user_code`, open `verification_uri`, poll, handle expiry and denial |

**No new external dependency.** The device flow is two HTTP calls against endpoints the crate
already talks to, and the polling loop is `tokio::time`.

**Two questions this node must answer against the live GitHub API before its green phase**, both
recorded rather than assumed:

1. **OAuth App or GitHub App?** An OAuth App's user token does not expire, so no refresh exchange is
   needed and nothing ever requires a client secret. A **GitHub App**'s user token expires in 8
   hours and its refresh exchange **does** post `client_secret` — which would re-introduce barrier 1
   through the back door, on the second day rather than the first. The recommendation is an
   **OAuth App**, and the device-flow response must be verified to confirm no `refresh_token` with
   an expiry arrives.
2. **Where does the public client id live** — compiled into the binary, or rendered into
   `desktop.yaml.production`? A client id is public by definition, so either is safe; rendering it
   keeps it visible and overridable, compiling it makes a hand-edited config unable to break sign-in.
   **Rendering it is the recommendation**, because `~/.tddy/desktop.yaml` is never overwritten and a
   baked-in id could not then be corrected.

**Unanalyzed packages this node lands in**: `tddy-desktop` and `tddy-web` have no
`docs/code-issues/` directory, which is not a clean bill of health — it means `/analyze-code-issues`
has never run there. Named, not blocking.

### User Impact

- **A fresh install signs in.** Open the app, see a short code, approve it on github.com, and the
  dashboard is live. No file is edited by hand.
- **No secret is shipped.** Nothing in the distributed application is confidential, which is what
  makes the download-and-run path honest rather than merely convenient.
- **The first account to sign in owns the install.** A second GitHub account is refused with the
  existing `permission_denied`, and adding one deliberately is `#keyring` 8/9's job. That is a
  narrower door than a served deployment's, and on a single-user desktop it is the right one.
- **Served deployments are unaffected.** They supply a client secret and keep the redirect flow.

## Implementation Plan

1. Open the base-URL seam in `RealGitHubProvider` and cover the six existing error returns —
   **before** any device-flow code.
2. `provider.rs` trait methods; `StubGitHubProvider` implementations.
3. `auth.proto` RPCs and messages; regenerate.
4. Device flow in `real.rs`: start call, then the polling state machine honouring `interval`,
   `slow_down`, `expired_token` and `access_denied`.
5. `auth.rs:109` gate: `client_id` alone → device-flow provider.
6. First-login enrolment and its persistence; the refusal path for a second login.
7. `tddy-web` sign-in screen.
8. `desktop.yaml.production` and the `tddy-desktop` docs that state the barriers.

**Verification is scoped** (`./test -p tddy-github -p tddy-daemon-auth -p tddy-daemon-kernel`, plus
the single `tddy-web` spec under change — never a full Cypress run, which is ~50 minutes over 207
specs). Whole-workspace green comes from CI.

## Acceptance Criteria

- [x] A daemon configured with **`client_id` only** registers `auth.AuthService` and serves
      `StartDeviceLogin`
- [x] `StartDeviceLogin` returns a `user_code` and a `verification_uri`, and **no client secret is
      sent** on the wire
- [x] `PollDeviceLogin` returns `pending` until approval, then a `v2` session token and a refresh
      token identical in shape to `ExchangeCode`'s
- [x] `PollDeviceLogin` honours `slow_down` by widening its interval, and surfaces `expired_token`
      and `access_denied` as distinct, actionable states
- [x] On a deployment with an empty `users:`, the **first** successful login is enrolled against the
      running OS user and persisted
- [x] A **second, different** GitHub login on an enrolled deployment is refused
      `permission_denied: user not mapped to OS user` — no fallback, no second enrolment
- [x] `os_user_for_github` is unchanged and has no default arm
- [x] A deployment with `client_id` **and** `client_secret` still serves the redirect flow unchanged
- [x] `RealGitHubProvider` takes a base URL, and `exchange_code`'s six error returns are covered
- [x] Only a login completed over the desktop's own window (`InProcess`) enrols. A login completed
      over any other transport on an unenrolled desktop is minted unmapped and writes nothing
      (`tddy-daemon/tests/first_login_enrolment_acceptance.rs`:
      `a_device_login_over_the_common_room_is_not_enrolled_on_an_unenrolled_desktop`,
      `a_device_login_over_the_agent_tool_socket_is_not_enrolled_on_an_unenrolled_desktop`,
      `a_desktop_still_enrols_its_window_login_after_a_common_room_login_was_not`,
      `a_desktop_still_enrols_its_window_login_after_an_agent_tool_socket_login_was_not`; the
      stamps are pinned by `tddy-tauri-rpc/tests/stamps_the_in_process_transport.rs` and
      `tddy-daemon-livekit/tests/forwarded_rpc_is_stamped_by_the_receiver.rs`)
- [x] An absent `auth_flow` means "no sign-in configured", an unknown value is an error, and a public
      client refuses `GetAuthUrl` with `failed_precondition`
      (`tddy-daemon-auth/tests/device_login_acceptance.rs`:
      `a_daemon_holding_only_a_public_client_id_refuses_to_begin_the_redirect_flow`,
      `a_daemon_holding_only_a_public_client_id_declares_the_device_flow`;
      `tddy-github/tests/real_provider_over_http.rs`:
      `a_public_client_hands_out_no_authorize_url_it_could_never_complete`;
      `tddy-daemon/tests/daemon_config_service.rs`: `declares_no_sign_in_flow_for_a_daemon_without_github`;
      `tddy-daemon/tests/server_options_acceptance.rs`:
      `omits_the_sign_in_flow_at_api_config_for_a_daemon_without_github`;
      `tddy-web/src/rpc/clientConfig.test.ts`: "reads a daemon that declares no auth_flow as serving
      no sign-in over HTTP" / "… over RPC", "keeps an auth_flow it does not recognise as
      unrecognised rather than guessing a flow"; `tddy-web/cypress/component/DeviceLoginAcceptance.cy.tsx`:
      "says the daemon has no GitHub sign-in configured, and offers neither flow, when it declares
      none", "names a sign-in flow it does not recognise as an error, and offers neither flow")
- [ ] A fresh `./install --desktop` reaches a signed-in dashboard with **no file edited by hand** —
      ⏸ deferred to the developer — 'I'll configure and test production myself'

## References

### Affected Features (Complete List)

- [Tddy Desktop](../tddy-desktop-tauri.md) — barriers 1 and 3
- [Cross-daemon session authentication](../../daemon/session-auth.md) — a second way to establish the same session
- [Daemon settings](../../daemon/daemon-settings.md) — reachable once sign-in works

### Stack

`#keyring` 2/9. Parent: 1/9 `signing-key` (barrier 2, and the signer this node's session is minted
with). Dependent: 8/9 `link-github`, which adds the deliberate second account this node refuses.

### Backlog

- ✅ [2026-09-18 — a desktop install configures no identity](../../../dev/todo/2026-09-18-desktop-install-configures-no-identity.md) — this node is what closes it
- ⚠ [2026-08-16 — models/agents open items at wrap](../../../dev/todo/2026-08-16-models-agents-open-items-at-wrap.md)
- ⚠ [2026-09-05 — Tauri desktop single-process daemon](../../../dev/todo/2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md)

### Code issues

- ✅ [`missing-tests-real-exchange-code`](../../../../packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md) — **claimed by this node**

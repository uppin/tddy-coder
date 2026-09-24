# 2026-09-24 — Zero-configuration GitHub sign-in on Tddy Desktop: the device flow and first-login enrolment

**Type:** Feature

`#keyring` 2/9 — PR [#509](https://github.com/uppin/tddy-coder/pull/509), base `master`, parent
`#keyring` 1/9 [#508](https://github.com/uppin/tddy-coder/pull/508) (merged). Dependent: `#keyring`
8/9 [#515](https://github.com/uppin/tddy-coder/pull/515) (`link-github`), which adds the deliberate
second GitHub account this node refuses.

## Summary

The daemon gains the **GitHub OAuth device flow** — the flow the GitHub CLI uses, which
authenticates with a public `client_id` alone — as `auth.StartDeviceLogin` / `auth.PollDeviceLogin`.
A `github:` block with a `client_id` and no `client_secret` registers it; `client_id` +
`client_secret` keeps the redirect flow for served deployments. A **desktop** deployment enrols its
first successful login from its own window against the OS user the application runs as, persists the
`users:` row into `~/.tddy/desktop.yaml`, and refuses every different login afterwards.
`os_user_for_github` is unchanged and has no default arm.

Whether a login came from the desktop's own window is decided by a **transport stamp** on every
request (`tddy_rpc::RequestTransport`), set by the host that received it and never by the caller:
`RequestMetadata` has no `Default`, `Request::new` is gone (`Request::direct` or a host's stamp
instead), and `ServerEngine::new` takes the transport its host names. That touches every crate that
builds a request or hosts an engine (~590 `Request::new` → `Request::direct`, ~90 metadata sites),
mechanically.

The dashboard reads the flow a daemon **declares** (`auth_flow` on `/api/config` and
`GetClientConfig`) and shows a device-code screen (`DaemonLoginScreen`, `DeviceLoginPanel`). The
Tauri application gains a strict Content Security Policy.

Together with #508 this removes the code side of all three things a fresh `./install --desktop` had
to have hand-edited. `desktop.yaml.production` still ships `github:` unset (see *Backlog*).

Where the end state is documented:

- [tddy-desktop-tauri.md](../../ft/desktop/tddy-desktop-tauri.md) — § Signing in, § What a fresh install still lacks, § Security
- [session-auth.md](../../ft/daemon/session-auth.md) — § Minting (both flows, the declared flow), § Who a login is
- [daemon-settings.md](../../ft/daemon/daemon-settings.md), [auth-livekit-services.md](../../ft/daemon/auth-livekit-services.md)
- [`tddy-github/docs/device-flow.md`](../../../packages/tddy-github/docs/device-flow.md) — the provider seam, both flows, the polling state machine, the stub
- [`tddy-rpc/docs/request-transport.md`](../../../packages/tddy-rpc/docs/request-transport.md) — `RequestTransport`, "hosts stamp, envelopes cannot", the stamping sites
- [`tddy-stdio/docs/stdio-endpoint.md`](../../../packages/tddy-stdio/docs/stdio-endpoint.md) — `from_duplex` takes its opener's transport
- [`tddy-daemon-auth/docs/auth-service.md`](../../../packages/tddy-daemon-auth/docs/auth-service.md) — which provider `github:` registers, `GitHubAuthFlow`, `FirstLoginEnrolment`
- [`tddy-daemon-kernel/docs/daemon-kernel.md`](../../../packages/tddy-daemon-kernel/docs/daemon-kernel.md) — `LiveUsers`, `enrol_first_login`
- [`tddy-daemon/docs/daemon-endpoint.md`](../../../packages/tddy-daemon/docs/daemon-endpoint.md) — the first-login admission in `runtime::build`, the declared flow
- [`tddy-desktop/docs/config-resolution-and-install.md`](../../../packages/tddy-desktop/docs/config-resolution-and-install.md) — the template, the Content Security Policy
- [`tddy-web/docs/daemon-sign-in.md`](../../../packages/tddy-web/docs/daemon-sign-in.md) — `auth_flow`, the device-code screen, `checkWholeSession`

| Package | Change |
|---|---|
| `tddy-github` | `GitHubOAuthProvider::start_device_login` / `poll_device_login`, `DeviceLoginStart`, `DeviceLoginPoll`; `authorize_url -> Result`. `RealGitHubProvider`: `oauth_base_url` / `api_base_url` (`new_with_base_urls`, `new_public`, `new_public_with_base_urls`), `RedirectClient`, `post_for_json` + `PostFailures`, `fetch_user`, the device flow with `DeviceAttempt`. `StubGitHubProvider`: device flow, `register_code_mappings`. `AuthServiceImpl`: the two handlers, `complete_login`, `LoginAdmission::admit(login, transport)`. `axum` dev-dependency |
| `tddy-service` | `auth.proto`: `StartDeviceLogin`, `PollDeviceLogin`, `DeviceLoginState`. `daemon_config.proto`: `GetClientConfigResponse.auth_flow` (field 10). The exec-tool tonic supplement stamps `Grpc` |
| `tddy-rpc` | `RequestTransport`; `RequestMetadata::over` (no `Default`, private transport); `Request::direct` (no `Request::new`); `ServerEngine::new(service, transport)` and `metadata_of`; `start_bidi_stream(.., metadata, ..)`; `From<tonic::Request>` stamps `Grpc` |
| `tddy-stdio` | `StdioEndpoint::from_duplex(.., transport)`; `from_process_stdio` / `from_child_stdio` stamp `Pipe` |
| `tddy-tauri-rpc`, `tddy-livekit`, `tddy-connectrpc`, `tddy-codegen` | the stamping hosts: `InProcess`, `LiveKit`, `Http`, `Grpc` (and the generated bidi handler takes its session's metadata) |
| `tddy-daemon-auth` | `github_provider_kind`, `GitHubAuthFlow`, `github_auth_flow`, `build_auth_entries_admitting`; the public-client arm; `first_login_admission.rs` (`FirstLoginEnrolment`); `register_stub_codes` deleted in favour of `register_code_mappings` |
| `tddy-daemon-kernel` | `live_users.rs` (`LiveUsers`), `first_login_enrolment.rs` (`enrol_first_login`, `EnrolmentRefusal`); `DaemonConfig.users: LiveUsers`; `os_user_for_github -> Option<String>` |
| `tddy-daemon` | `runtime::first_login_enrolment` and `this_process_os_user`; the embedded-host refusal; `auth_flow` at `/api/config` and in `GetClientConfig`; `UpdateConfig` rewrites under `LiveUsers::while_rewriting_config_file` |
| `tddy-coder` | `standalone_auth_provider` / `StandaloneAuthProvider`: the one decision `build_auth_service_entry` and `build_client_config` read, so `auth_flow` is `"redirect"` exactly when an entry is registered; `auth_callback_on`, `auth_service_entry_for` |
| `tddy-bsp` | `DaemonBspService`'s relays pass the inbound request's metadata through (`Request::with_metadata`) |
| `tddy-web` | `ClientConfig.authFlow` (`AuthFlowDeclaration`); `DaemonLoginScreen` (extracted from `index.tsx`), `DeviceLoginPanel`; `useAuth`'s device flow and `checkWholeSession`; `GITHUB_BUTTON_CLASS_NAME` |
| `tddy-desktop` | `src-tauri/tauri.conf.json`: a strict CSP replaces `"csp": null` |
| `tddy-daemon-livekit` | test only: `forwarded_rpc_is_stamped_by_the_receiver.rs`; `uuid` dev-dependency |
| every other crate that built a request | `Request::direct` / an explicit transport; no behaviour change |

**Dependencies.** No new crate in `Cargo.lock`. Two dev-dependency edges: `axum` on `tddy-github`
(to serve a GitHub on loopback) and `uuid` on `tddy-daemon-livekit` (developer-approved).

## Decisions

- **No fallbacks, no backward compatibility** (developer: "don't leave any fallbacks. I'll rollout
  all the desktop instances"). An absent `auth_flow` is "no sign-in configured", never the redirect
  flow; an unknown one is an error. A public client refuses `GetAuthUrl` as well as `ExchangeCode`
  (`failed_precondition`). `poll_device_login` errors on an interval-less `slow_down` for a code it
  has no open attempt for, rather than inventing GitHub's documented 5 s. The dashboard ends a device
  attempt `failed` on a grant or `SLOW_DOWN` without a positive interval. `runtime::build` refuses an
  embedded host serving sign-in to an empty `users:` with no config file, rather than assembling a
  desktop that can never enrol.
- **Enrolment is once, and only from the window.** `os_user_for_github` gains no default arm;
  enrolment writes the row the unchanged lookup then finds, on a deployment that has none. It is
  decided by the **host-stamped transport** of the completing call (option 2 of three weighed; the
  alternatives were per-transport rosters and refusing enrolment while a common room is configured).
  `FirstLoginEnrolment` matches the transport exhaustively: only `InProcess` enrols; any other on an
  unenrolled desktop is admitted unmapped and writes nothing. A second account is refused, and adding
  one is #515's. Forwarding needs no special case: the receiving daemon stamps `LiveKit`.
- **The sign-in flow is declared.** `github_auth_flow` reads the same `github_provider_kind` the
  builder does, and feeds both `/api/config` and `GetClientConfig`.
- **A strict CSP** on the Tauri application, because `InProcess` means "the person at the machine"
  and script injected before sign-in could otherwise enrol. No `unsafe-eval`, no inline script,
  `'unsafe-inline'` only in `style-src-elem`; no `devCsp`.
- **The whole-session guard.** Both web flows go through one `checkWholeSession`: a session missing
  its user, session token or refresh token is refused, names the missing part, and stores nothing.
- **OAuth App, client id rendered into `desktop.yaml.production`** (developer). An OAuth App's user
  token does not expire, so no refresh exchange ever needs a secret; a rendered id stays visible and
  correctable, since `~/.tddy/desktop.yaml` is never overwritten.
- **The base-URL seam first**, before the device flow doubled the provider's network code.
- **No restructure of `auth.rs` or `config.rs`** in this stack (`## Boundaries`, two independent
  records).

## Backlog

- **Kept, narrowed** — `2026-09-18-desktop-install-configures-no-identity.md` ("A desktop install
  configures no identity, so it has no sessions"). The code side of all three requirements is closed
  (#508 and this PR). What remains is the developer's: render the OAuth App's public `client_id` into
  repo-root `desktop.yaml.production` (and rewrite its `github:` / `users:` comments), and verify a
  fresh `./install --desktop` signs in with no file edited by hand, including the live-API check that
  the device-flow token carries no expiring `refresh_token`. Deleted when the developer confirms.
- **Added** — `2026-09-24-keyring-desktop-login-grew-thirteen-over-budget-files.md`: the thirteen
  files this PR grew past (or further past) the 500-line production budget, deferred with the
  developer's consent ("defer all, record"), to be split after the `#keyring` stack lands. It also
  records the file-length gate's `cfg(any(feature, test))` undercount.
- **Extended** — `2026-09-05-from-2026-09-05-tauri-desktop-single-process-daemon.md`: its
  comment-loss item names the second writer, `enrol_first_login`, which strips every YAML comment on
  a desktop's first sign-in. One fix (a comment-aware YAML editor) for both writers.
- Not fixed, recorded as touched: `2026-08-16-models-agents-open-items-at-wrap.md`.

## Code issues closed — final measurements

**`packages/tddy-github/docs/code-issues/missing-tests-real-exchange-code.md`** — claimed by #509,
**closed**.

| | At detection (2026-09-19) | At wrap (2026-09-24) |
|---|---|---|
| Production lines of `RealGitHubProvider::exchange_code` | 71 | **26**, over the shared `post_for_json` / `fetch_user` |
| Outbound HTTP calls | 2 | 2 |
| Hardcoded absolute hosts | 2 | **0** — `oauth_base_url` / `api_base_url` fields, production defaults `GITHUB_OAUTH_BASE_URL` / `GITHUB_API_BASE_URL`, test seam `new_with_base_urls` |
| Error returns exercised | 0 of 6 (as recorded) | **all 7 original**: forged state; token-leg transport, status, parse; user-leg transport, status, parse. The record said 6; the seventh is the user-leg transport failure. Plus the public-client refusal: 8 of 8 |
| Tests entering the function | 0 | **5** test functions in `packages/tddy-github/tests/real_provider_over_http.rs`, 9 calls |

Measured on the receiver type: `AuthServiceImpl::exchange_code`, the service over
`StubGitHubProvider`, is a different function sharing only the name, as the record warned.

**`packages/tddy-coder/docs/code-issues/complexity-run-build-auth-service-entry.md`** — **closed**.
`build_auth_service_entry` (`packages/tddy-coder/src/run.rs`): 65 lines, nesting 6, 8 branch lines,
0 early exits at merge-base `4e7157d2` → **36 lines, nesting 4, 5 branch lines, 1 early exit, 1
parameter** at wrap, by hand structural scan. Closed by `standalone_auth_provider`,
`auth_callback_on`, `auth_service_entry_for` and `StubGitHubProvider::register_code_mappings`.

**Moved, not closed** — `complexity-auth-build-auth-entries-with.md` →
`complexity-auth-build-auth-entries-admitting.md` (`packages/tddy-daemon-auth`). The body moved into
`build_auth_entries_admitting`, and grew 92 → **106** lines (the enrolment admission and the public
provider arm): **regressed**, deferred with the file-length consent.

## Code-issue reconciliation (step 7.5)

76 open records in the packages this PR touched: **55 untouched**, **20 touched**:

- 1 deleted (`complexity-run-build-auth-service-entry`), plus the claimed one
  (`missing-tests-real-exchange-code`);
- 1 moved (`complexity-auth-build-auth-entries-with` → `…-admitting`);
- 1 partially fixed: `complexity-router-handle-rpc` (`tddy-connectrpc`), 147 → 141 lines;
- 8 regressed, 7 of them deferred with consent;
- 9 unchanged.

6 alert-only `oversized-file` records added: `tddy-daemon-rpc` (`oversized-file-pr-stack-ports`,
`oversized-file-project-coordinate-handlers`), `tddy-host-service` and `tddy-worktree-service`
(`oversized-file-service`), `tddy-screen-sharing` (`oversized-file-screen-sharing-service`) and
`tddy-session-tool-client` (`oversized-file-lib`).

## Deferred and open risks

| # | State | Finding |
|---|---|---|
| V2 | open, recorded | `enrol_first_login` strips every YAML comment from `~/.tddy/desktop.yaml` on the first sign-in, the rendered header included (backlog entry above) |
| V6 | open, recorded | cloning a `DaemonConfig` shares `users:` (`LiveUsers`) — a sharp edge, documented at the field |
| V7 | open | `DeviceLoginPanel`'s `target="_blank"` inside the Tauri webview relies on `tauri-plugin-opener`; unconfirmed on the desktop |
| V8 | deferred to the developer | the OAuth App's device-flow token not yet checked against the live API for an expiring `refresh_token` |
| V11 | open, recorded | enrolment reads the *completing* poll's transport; the `StartDeviceLogin` that issued the code is not bound to it. Not exploitable (a device code reaches only its starter); binding would be defence in depth |
| V12 | implemented, **unverified at runtime** | the CSP has not been run in a launched `custom-protocol` build (terminal WASM, IPC, LiveKit `ws://`, console errors) |
| V15 | open, pre-existing | an unreachable daemon or non-OK `/api/config` renders the standalone connection form; on a desktop a failed `GetClientConfig` would too |
| V17 | open, recorded | `admit` does synchronous file I/O under a `std::sync::Mutex` on an async RPC task, once per deployment |
| M8 | deferred to the developer | `desktop.yaml.production` unchanged — `github:` unset, comments still describing a secret and a hand-written `users:` |
| — | deferred to the developer | the fresh-install acceptance criterion: `./install --desktop` reaching a signed-in dashboard with no file edited by hand |

**Code quality: C.** The remaining must-refactor is the recorded `build_auth_entries_admitting`
(106 lines, `tddy-daemon-auth/src/auth.rs`), whose split waits for #keyring 3/9 and the post-stack
restructure.

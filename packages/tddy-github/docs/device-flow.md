# GitHub sign-in — the device flow and the provider seam

**Modules**: [`src/provider.rs`](../src/provider.rs) (the trait and its domain types),
[`src/real.rs`](../src/real.rs) (`RealGitHubProvider`), [`src/stub.rs`](../src/stub.rs)
(`StubGitHubProvider`), [`src/auth_service.rs`](../src/auth_service.rs) (`AuthServiceImpl`)

`GitHubOAuthProvider` is how a daemon asks GitHub who an operator is. It carries **two sign-in
flows**, and a deployment chooses one by what it is configured with:

| Flow | Methods | Needs | Who uses it |
|---|---|---|---|
| **Redirect** | `authorize_url`, `exchange_code` | `client_id` **and** `client_secret`, plus a callback URL | a served deployment behind a real callback |
| **Device** | `start_device_login`, `poll_device_login` | `client_id` alone | Tddy Desktop, and any public client |

The device flow ([RFC 8628](https://www.rfc-editor.org/rfc/rfc8628)) is the one the GitHub CLI
uses. The operator is shown a short code, types it at GitHub's verification page and approves
there, while the daemon polls GitHub until the approval arrives. **No request in the device flow
carries a client secret**, which is what lets a downloadable application sign in without shipping
one. Neither flow is a fallback for the other.

Both flows end in the same place: an access token and the `GitHubUser` it belongs to, handed to
`AuthServiceImpl::complete_login`, which admits, retains and mints exactly as it does for either
flow — see [Completing a sign-in](#completing-a-sign-in).

## The trait

```rust
fn authorize_url(&self) -> Result<(String, String), String>;
async fn exchange_code(&self, code: &str, state: &str) -> Result<(String, GitHubUser), String>;
async fn start_device_login(&self) -> Result<DeviceLoginStart, String>;
async fn poll_device_login(&self, device_code: &str) -> Result<DeviceLoginPoll, String>;
fn issues_usable_access_token(&self) -> bool;
```

`authorize_url` returns `Result`: a provider that cannot complete the redirect flow at all (a
public client) refuses to hand out a URL the operator could follow to GitHub and back only for the
exchange to fail.

**`DeviceLoginStart`** is what GitHub answers a start with:

| Field | Meaning |
|---|---|
| `device_code` | the daemon's half — sent back on every poll, never shown |
| `user_code` | the operator's half — the only thing they type |
| `verification_uri` | where they type it |
| `expires_in_seconds` | how long the codes stay valid |
| `interval_seconds` | GitHub's **minimum** seconds between polls |

**`DeviceLoginPoll`** is one enum rather than an `Option` plus error strings, because the caller's
next move differs for every variant:

| Variant | Next move |
|---|---|
| `Pending` | poll again after the interval |
| `SlowDown { interval_seconds }` | adopt the wider interval, then poll again |
| `Denied` | the operator refused; the attempt is over and is not retried silently |
| `Expired` | the codes outlived their window; a new start is needed |
| `Complete { access_token, user }` | approved, in the shape `exchange_code` returns |

The `Err` arm of `poll_device_login` is a transport or protocol failure only — GitHub unreachable,
an answer that does not parse, an error code none of the variants names.

## `RealGitHubProvider`

### Constructors

| Constructor | Client | Hosts |
|---|---|---|
| `new(client_id, client_secret, redirect_uri)` | confidential | GitHub's own |
| `new_with_base_urls(client_id, client_secret, redirect_uri, oauth_base_url, api_base_url)` | confidential | the given ones |
| `new_public(client_id)` | public | GitHub's own |
| `new_public_with_base_urls(client_id, oauth_base_url, api_base_url)` | public | the given ones |

A **confidential** client holds a `RedirectClient { client_secret, redirect_uri }` and signs in by
either flow (the device flow never sends the secret). A **public** client (RFC 6749 §2.1) holds
`redirect_client: None`: no secret and no callback, because the device flow returns to none. Its
`authorize_url` and `exchange_code` refuse, both with a message ending "sign in with the device
flow". `exchange_code` refuses **before** checking the state, since a public client issues none and
would otherwise report every exchange as a forged state.

### The base-URL seam

Every request is built from two fields rather than a literal:

| Field | Production default | Serves |
|---|---|---|
| `oauth_base_url` | `GITHUB_OAUTH_BASE_URL` = `https://github.com` | `/login/oauth/authorize`, `/login/oauth/access_token` (`ACCESS_TOKEN_PATH`), `/login/device/code` (`DEVICE_CODE_PATH`) |
| `api_base_url` | `GITHUB_API_BASE_URL` = `https://api.github.com` | `/user` |

They are two fields because they are two hosts. A trailing `/` is trimmed. `new_with_base_urls` and
`new_public_with_base_urls` are ordinary production constructors, not `cfg(test)` branches: a test
serves both hosts on loopback and passes their addresses, and GitHub Enterprise is the same
substitution. The seam is to the **host** only. It does not change `issues_usable_access_token`,
which stays `true`: a token this provider returns is whatever the host it was pointed at granted.

### One POST, worded per call

`post_for_json::<T>(path, body, &PostFailures)` is the provider's only POST: `{oauth_base_url}{path}`,
`Accept: application/json`, a JSON body, a success-status check, then a parse into `T`. Each caller
passes a `PostFailures { request, status, response }` constant that words its three failures:

| Constant | Used by | Request / status / parse wording |
|---|---|---|
| `TOKEN_EXCHANGE_FAILURES` | `exchange_code` | `token exchange request failed: …` / `token exchange failed with status: …` / `failed to parse token response: …` |
| `DEVICE_CODE_FAILURES` | `start_device_login` | `device code request failed…` / `…with status: …` / `failed to parse device code response: …` |
| `DEVICE_POLL_FAILURES` | `poll_device_login` | `device login poll failed…` / `…with status: …` / `failed to parse device login poll response: …` |

`fetch_user(access_token)` is the shared `GET {api_base_url}/user` both flows finish with, and has
its own three failures (`user info request failed: …`, `… with status: …`, `failed to parse user
response: …`). GitHub's nullable display name becomes an empty string.

The scope every sign-in asks for is `SCOPES` = `read:user repo`: `read:user` identifies the
operator, and `repo` lets the retained token read pull requests on a private repository. The
authorize URL carries it as `read:user%20repo`.

### The device flow's requests

| Step | Request | Body |
|---|---|---|
| start | `POST /login/device/code` | `client_id`, `scope` |
| poll | `POST /login/oauth/access_token` | `client_id`, `device_code`, `grant_type = urn:ietf:params:oauth:grant-type:device_code` |
| on approval | `GET /user` on the API host | — (`Authorization: Bearer <token>`) |

The redirect flow's exchange posts `client_id`, `client_secret` and `code` to the same
`/login/oauth/access_token`. That is the only request in either flow that names the secret.

### The polling state machine

GitHub answers a poll `200 OK` whether or not the code is approved, so the outcome is read from
which fields of the answer are set (`device_poll_outcome`):

| `access_token` | `error` | Result | The attempt |
|---|---|---|---|
| set | — | `Complete`, after `fetch_user` | forgotten |
| — | `authorization_pending` | `Pending` | kept |
| — | `slow_down` | `SlowDown { interval }` — see below | kept, interval updated |
| — | `expired_token` | `Expired` | forgotten |
| — | `access_denied` | `Denied` | forgotten |
| any | any other code | `Err("device login failed: <error>[: <error_description>]")` | forgotten |
| — | — | `Err("failed to parse device login poll response: neither an access token nor an error")` | kept |

**`slow_down` widening** (`widened_interval`):

- GitHub names an interval → that interval is returned, and remembered for the attempt.
- GitHub names none, and the attempt is open → the attempt's last interval plus
  `SLOW_DOWN_WIDENING_SECONDS` (5), remembered. RFC 8628 §3.5 requires a client told `slow_down` to
  add 5 seconds for this and every later request, so a second `slow_down` widens from the first
  (5 → 10 → 15).
- GitHub names none, and there is **no open attempt** for that code (never started here, or past its
  window) → `Err("GitHub asked to slow down polling for a device code with no open attempt on this
  daemon …")`. There is no interval to widen, and inventing one would tell the client a number
  GitHub never said.

**The attempt window.** `device_attempts: Mutex<HashMap<device_code, DeviceAttempt>>` holds one
`DeviceAttempt { interval_seconds, expires_at }` per code this provider started, with `expires_at`
set from `expires_in`. It exists only so an interval-less `slow_down` has something to widen, and it
is bounded: every start and every poll first prunes the attempts whose window has closed
(`open_device_attempts`), and a terminal answer removes its own. An attempt the operator abandoned
therefore does not stay, even if nothing is ever started again.

The provider keeps no other device state. The device code is the client's to hold and present on
each poll.

## `StubGitHubProvider`

The in-memory provider the acceptance suites and demo deployments use. It makes no HTTP call.

- `register_code(code, user)` maps an authorization code to a user; `register_code_mappings(codes)`
  registers every `code:login` pair in a comma-separated string — the shape `--github-stub-codes`
  and `github.stub_codes` take. An entry without `:` is skipped, and nothing is trimmed.
- **Device flow:** each `start_device_login` numbers itself, returning `stub-device-code-<n>`, user
  code `STUB-<nnnn>`, verification URI `<authorize_base_url>/login/device`, a 900-second window
  (`STUB_DEVICE_LOGIN_EXPIRES_IN_SECONDS`, GitHub's own fifteen minutes) and a 1-second interval.
  The first poll answers `Pending` (`STUB_DEVICE_LOGIN_PENDING_POLLS` = 1), the next `Complete` as
  the user of the **first** code registered, and the code is then spent. An unknown device code, or
  no registered user, is an `Err`. The stub never answers `SlowDown`, `Denied` or `Expired`; those
  are covered against `RealGitHubProvider` over loopback HTTP.
- `issues_usable_access_token()` is `false`: the stub's token is synthetic, so a stub login retains
  no GitHub credential by construction.

The stub serves both flows. A daemon running it declares the redirect flow to its dashboard.

## Completing a sign-in

`AuthServiceImpl` serves both flows over `auth.AuthService` (`tddy-service/proto/auth.proto`):
`GetAuthUrl` / `ExchangeCode` for the redirect flow, `StartDeviceLogin` / `PollDeviceLogin` for the
device flow. `GetAuthUrl` maps a provider refusal to `failed_precondition`. `StartDeviceLogin`
remembers nothing. `PollDeviceLogin` maps each `DeviceLoginPoll` to a `DeviceLoginState`
(`PENDING`, `SLOW_DOWN` with `interval_seconds`, `DENIED`, `EXPIRED`, `COMPLETE`), and on
`COMPLETE` sets `session_token`, `user` and `refresh_token` exactly as `ExchangeCodeResponse` does.
A provider `Err` becomes `internal`.

`ExchangeCode` and a completing `PollDeviceLogin` both go through one `complete_login(access_token,
user, transport)`, so the two flows cannot drift into different sessions or different retention
rules:

1. **Admission** — when a `LoginAdmission` is set (`with_login_admission`), `admit(github_login,
   transport)` is asked first. A refusal fails the login and leaves nothing behind. `transport` is
   the completing request's `RequestMetadata::transport()`, as the host that received it stamped
   it — see [`tddy-rpc` request transport](../../tddy-rpc/docs/request-transport.md). The desktop's
   implementation is `tddy-daemon-auth`'s `FirstLoginEnrolment`.
2. **Retention** — a usable access token is put into the `GitHubTokenStore`; a failed put fails
   the login.
3. **Minting** — the `v2` access and refresh tokens ([session-token.md](./session-token.md)).
   Without a signer the login fails `failed_precondition`.

## Tests

| Suite | Covers |
|---|---|
| `tests/real_provider_over_http.rs` | `RealGitHubProvider` against a GitHub served on loopback by `axum` (a dev-dependency only). The device flow's start, each poll state, `slow_down` widening (named, unnamed, twice in a row, and refused for want of an open attempt), an unknown error code, the attempt window's pruning, a full public-client sign-in hitting `/login/device/code`, `/login/oauth/access_token` and `/user` in order, and that **no part of any device-flow request — path, query, headers or body — carries the secret**. `exchange_code`'s success path and every one of its error returns: a forged state, the token leg's transport, status and parse failures, the user leg's transport, status and parse failures, and a public client's refusal. Its `authorize_url` refusal too |
| `src/auth_service.rs` (inline) | the proto mapping of `SlowDown`, `Denied` and `Expired` to `(state, interval_seconds)` through a scripted provider; admission being told the transport a redirect login and a device login each completed over; a refused admission failing the login with the daemon's reason |
| `tests/github_token_retention_acceptance.rs` | retention through `ExchangeCode`, and the authorize URL's scope |

`AuthServiceImpl::exchange_code` (the service) and `RealGitHubProvider::exchange_code` (the
network) share a name and nothing else. Coverage of one is not coverage of the other.

## Related

- [session-token.md](./session-token.md) — the token a completed sign-in mints
- [`tddy-daemon-auth` auth-service.md](../../tddy-daemon-auth/docs/auth-service.md) — which provider
  a daemon's `github:` block registers, and first-login enrolment
- [Cross-daemon session authentication](../../../docs/ft/daemon/session-auth.md)
- [changesets/](./changesets/)

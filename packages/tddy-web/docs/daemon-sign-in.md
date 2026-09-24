# Daemon sign-in (`src/components/DaemonLoginScreen.tsx`, `src/components/DeviceLoginPanel.tsx`, `src/hooks/useAuth.ts`)

A daemon-mode page that is signed out shows `DaemonLoginScreen`. What it offers is decided by the
daemon, never guessed by the page: the daemon **declares** which GitHub sign-in flow it serves, and
the screen offers exactly that one.

Feature docs: [Cross-daemon session authentication](../../../docs/ft/daemon/session-auth.md),
[Tddy Desktop](../../../docs/ft/desktop/tddy-desktop-tauri.md).

## The declaration: `auth_flow`

Both client-config paths carry it — `GET /api/config` in a browser (`auth_flow`), and
`DaemonConfigService.GetClientConfig` where there is no HTTP origin (`GetClientConfigResponse.auth_flow`).
`src/rpc/clientConfig.ts` reads either into `ClientConfig.authFlow`, an `AuthFlowDeclaration`
(`authFlowOf`):

| Declared | `authFlow` | The screen offers |
|---|---|---|
| `"redirect"` | `"redirect"` | `GitHubLoginButton` — `GetAuthUrl`, GitHub, `/auth/callback`, `ExchangeCode` |
| `"device"` | `"device"` | `DeviceLoginPanel` — `StartDeviceLogin`, then `PollDeviceLogin` |
| absent | `"none"` | **neither**: "This daemon has no GitHub sign-in configured." |
| anything else | `{ unrecognised: value }` | **neither**: an error naming the value |

The declaration is always stated. Absence is "this daemon serves no sign-in", never the redirect
flow, and an unknown value is an error, not a flow the page falls into. Offering the other flow would
fail against that daemon. `App` (`src/index.tsx`) types this into its state: a daemon-mode config
always carries its `authFlow`, so the sign-in screen cannot be rendered from a guessed one.

A daemon that cannot be reached, or answers `/api/config` with a non-OK status, is read as not a
daemon at all and gets the standalone connection form (`daemonMode: false`). On a desktop a failed
`GetClientConfig` would therefore show that form rather than an error.

## The device-code flow

`DeviceLoginPanel` reads `deviceLogin` and `startDeviceLogin` from the shared auth context
(`useAuthContext`). `deviceLogin` is a `DeviceLogin`:

| `phase` | Shown |
|---|---|
| `idle` | "Sign in with GitHub" |
| `starting` | the same button, disabled |
| `awaiting-approval` | the verification URI as a link (`target="_blank"`) and the user code, "Waiting for approval at GitHub…" |
| `denied` | "Sign-in was denied at GitHub." and "Get a new code" |
| `expired` | "The sign-in code expired before it was approved." and "Get a new code" |
| `failed` | the error, and "Get a new code" |

`startDeviceLogin` calls `StartDeviceLogin`, then polls `PollDeviceLogin` no faster than the
interval it was granted. Each answer becomes one `DevicePollStep` (`devicePollStep`, a pure function):

| Answer | Step |
|---|---|
| `PENDING` | poll again after the current interval |
| `SLOW_DOWN` | poll again after the interval the answer names, which every later poll keeps |
| `COMPLETE` with a whole session | adopt it — the operator is signed in |
| `COMPLETE` missing any part | `failed`, naming the missing part |
| `DENIED` / `EXPIRED` | the matching phase |
| any other state | `failed`, naming the state |

**An interval is a protocol fact, not a default.** `intervalMsOf` maps a non-positive interval to
`null`, and a grant or a `SLOW_DOWN` without a positive interval ends the attempt `failed` rather than
polling with no delay. A failed `StartDeviceLogin` or `PollDeviceLogin` ends it `failed` with the
error's own message.

**Only the latest attempt acts.** Starting again, or unmounting, advances the attempt's generation
and clears its timer, so a poll still in flight for an old device code lands on a dead attempt and
schedules nothing.

## A partial session is refused, whichever flow produced it

`checkWholeSession` is the one guard both flows go through. A session the daemon minted — by
`ExchangeCode` or by a `COMPLETE` poll — is **whole** only when it has a `user`, a non-empty session
token and a non-empty refresh token. proto3 leaves an unset field as an absent message or an empty
string, and either is a missing part, never one to fill with a default.

On a partial session neither flow adopts anything: nothing is stored, the operator stays signed out,
and the error names every missing part ("… without a whole session (no user, no refresh token)").
The redirect path, like every failed exchange there, also clears any session stored earlier; the
device path leaves storage as it was. `adoptSession` accepts only a `WholeSession`.

## Tests

| Spec | Covers |
|---|---|
| `cypress/component/DeviceLoginAcceptance.cy.tsx` | the device-code flow over an in-memory transport (`cypress/support/rpc/deviceLoginBackend.ts`), with `cy.clock`-driven polling: the code and link shown, pending then approval, slow-down widening, denial, expiry, a fresh code, interval-less grants and slow-downs failing, a `COMPLETE` missing each part, and the `none` / unrecognised declarations offering neither flow |
| `cypress/component/RedirectLoginAcceptance.cy.tsx` | `ExchangeCode` without a user, session token or refresh token each ending sign-in with the exact error, storing nothing, and leaving the operator signed out |
| `src/rpc/clientConfig.test.ts` | `auth_flow` read over HTTP and over RPC: each flow, absence as `"none"`, an unknown value kept as unrecognised |

Run a spec alone; never the full `cypress:e2e`.

## Related

- [`tddy-daemon-auth` auth-service.md](../../tddy-daemon-auth/docs/auth-service.md#which-sign-in-github-registers) — which flow a daemon's `github:` block makes it declare
- [`tddy-github` device-flow.md](../../tddy-github/docs/device-flow.md) — the provider behind both flows
- [changesets/](./changesets/)

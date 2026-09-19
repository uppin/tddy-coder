# missing-tests: RealGitHubProvider::exchange_code

**Location:** `packages/tddy-github/src/real.rs:59` — `RealGitHubProvider::exchange_code`
**Category:** missing-tests
**Detected:** 2026-09-19 by structural audit
**Metrics:** **71 production lines** · **2 outbound HTTP calls** · **6 error returns, 0 exercised** · 2 hardcoded absolute hosts · 0 tests enter the function
**Coverage:** **not measured** — `tddy-tools analyze coverage` was not run for this crate; the count above is a call-graph check, so "0 tests enter it" is a reference fact, not a coverage tier
**Restructure:** not required — ordinary work (a base-URL seam, then tests)
**Status:** Open — **unclaimed**
**Verified:** ✅ hand-verified 2026-09-19 — see *Verified by hand*

## Measurement history

| Run | Prod lines | HTTP calls | Error returns exercised | Note |
|---|---|---|---|---|
| 2026-09-19 | 71 | 2 | 0 of 6 | first detection |

## What the tool found

`RealGitHubProvider` has four public entry points. Three are reached by a test; this one is not.

```
$ grep -rn "RealGitHubProvider" --include="*.rs" packages/ | grep -v src/real.rs
packages/tddy-daemon-auth/src/auth.rs:114      production
packages/tddy-coder/src/run.rs:1185            production
packages/tddy-github/tests/github_token_retention_acceptance.rs:267   test — authorize_url only
```

The single test construction (`github_token_retention_acceptance.rs:264`,
`asks_github_for_the_repo_scope_as_well_as_the_users_identity`) builds the provider and asserts on
`authorize_url`'s query string. It never calls `exchange_code`.

**Why nothing calls it is the finding.** The function posts to a hardcoded
`https://github.com/login/oauth/access_token` (`real.rs:67`) and then gets a hardcoded
`https://api.github.com/user` (`real.rs:91`). There is no base URL on the struct and no parameter
carrying one, so a test cannot point it at a local server without editing production code. The
function is **untestable by construction**, not merely untested.

The six unexercised failure paths are: invalid/expired state (`:61`), token request transport
failure (`:76`), non-2xx token status (`:79`), token body parse failure (`:88`), user request
transport failure (`:101`), non-2xx user status (`:105`), user body parse failure (`:111`).

**Not measured:** no CRAP score. Coverage was not collected for `tddy-github`, so this record
carries reference counts rather than a coverage tier. Running
`tddy-tools analyze coverage --path packages/tddy-github` would replace the estimate with a number.

## Why it matters here

Every message this function produces is one an operator reads while locked out, and none of them has
ever been asserted. `exchange_code`'s `Err` is what `AuthServiceImpl::exchange_code` turns into a
failed login, and `token_store.rs`'s own doc comment explains the stake: a login that half-succeeds
leaves the operator "appearing signed in while every GitHub-backed read reports itself unavailable,
and re-authenticating, the one remedy, is the one action they have no reason to attempt."

The untestability also compounds rather than staying still. `#keyring` 2/9 adds the **device
authorization flow** to this same provider — `POST /login/device/code` plus a polling
`POST /login/oauth/access_token` with its own `authorization_pending`, `slow_down`, `expired_token`
and `access_denied` states. Written against the shape this file already has, that lands roughly
another hundred lines of network code with the same zero-test property, and the polling state
machine is materially harder to get right than the two-call exchange is.

## What would close it

Ordinary work, in this order:

1. Give `RealGitHubProvider` an injectable base URL for each of the two hosts — they are distinct
   (`github.com` for OAuth, `api.github.com` for the REST API), so one field cannot cover both.
   Default to the real hosts so no caller changes.
2. Test the six failure paths and the success path against a local HTTP server.

**A note for whoever does it.** `issues_usable_access_token` exists because a synthetic stub token
must never be retained; the base-URL seam must not become a second way to get a non-GitHub token
treated as usable. Keep the seam to the *host*, not to the provider's honesty about its own token.

Do this **before or inside** `#keyring` 2/9 rather than after. Adding the device flow first means
writing the seam twice.

## Verified by hand

**2026-09-19.** Read `real.rs:45-129` in full. Confirmed:

- `authorize_url` is reached by exactly one test; `exchange_code` by none. Checked by listing every
  reference to `RealGitHubProvider` outside its own file (3 hits: 2 production, 1 test) and reading
  the test body.
- Both URLs are string literals inside the function with no configuration path. Confirmed there is
  no base-URL field by reading the struct at `:11` and its constructor at `:33`.
- Counted the error returns by reading the body, not by grep.

**What an automated pass would have got wrong here.** A plain name grep suggests `exchange_code` is
well covered, because `AuthServiceImpl::exchange_code` — a *different* function, on the service, over
`StubGitHubProvider` — has six tests across `auth_service.rs` and
`github_token_retention_acceptance.rs`. The two share a name and nothing else. The tested one never
touches the network; this one is the network. Any re-run of this record must distinguish them by
receiver type, not by name.

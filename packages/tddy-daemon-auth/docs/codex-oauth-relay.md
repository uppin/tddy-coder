# `codex_oauth_relay` (tddy-daemon-auth)

## Overview

The **`codex_oauth_relay`** module implements **HTTPS** authorize URL validation, **`BROWSER` argv** parsing into **`CodexOAuthAuthorizeCapture`**, and **OAuth callback** query parsing into **`CodexOAuthCallbackDelivery`**. It contains no network clients; callers integrate with RPC, hooks, and process forwarding.

## Public API

| Item | Role |
|------|------|
| **`CodexOAuthHostAllowlist`** | Default allowlist for Codex/OpenAI OAuth hosts; **`contains_host`** is case-insensitive. |
| **`validate_codex_oauth_authorize_url`** | Validates scheme, allowlisted host, and optional session correlation. |
| **`dispatch_browser_open_capture`** | Async; finds first **`https`** URL in **`browser_argv`**, validates for **`session_id`**, returns capture struct. |
| **`relay_oauth_callback_to_registered_listener`** | Async; parses **`callback_url`** query pairs into **`CodexOAuthCallbackDelivery`**. |

Errors surface as **`CodexOAuthRelayError::Validation(...)`** with **`CodexOAuthValidationError`** variants (**`SchemeNotHttps`**, **`HostNotAllowed`**, **`CorrelationMismatch`**, **`NoHttpsAuthorizeUrlInBrowserArgv`**).

## Logging

**`log::debug!`** and **`log::info!`** use target **`tddy_daemon::codex_oauth`**. Logs avoid printing full authorize URLs or OAuth secrets.

**The target names `tddy_daemon`, not this crate, and that is deliberate.** A log target is an operator's `RUST_LOG` filter, so renaming it to match the crate would silently stop every filter that already selects it. A fleet-wide rename of the daemon's targets is its own change with its own release note.

## Tests

```bash
cargo test -p tddy-daemon-auth codex_oauth_relay::tests -- --test-threads=1
cargo test -p tddy-integration-tests --test codex_oauth_web_relay_acceptance -- --test-threads=1
```

## Related package docs

- **[oauth-loopback-tunnel.md](./oauth-loopback-tunnel.md)** — operator TCP + **`StreamBytes`** bridge, in this crate

## Feature documentation

- **[Codex OAuth web relay](../../../docs/ft/web/codex-oauth-web-relay.md)**
- **[Codex OAuth relay (daemon)](../../../docs/ft/daemon/codex-oauth-relay.md)**

## Related

- **[auth-service.md](./auth-service.md)** — the crate this module belongs to
- **[changesets/](./changesets/)**

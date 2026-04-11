# Codex OAuth relay (daemon)

## Role

The **`tddy_daemon::codex_oauth_relay`** module validates **HTTPS** authorize URLs for Codex/OpenAI OAuth, shapes **`BROWSER`** hook argv into **`CodexOAuthAuthorizeCapture`**, and parses **OAuth callback** URLs into **`CodexOAuthCallbackDelivery`**. Higher layers use these types for session-scoped events and for forwarding query parameters toward the Codex CLI listener. Session metadata for pending OAuth (**`CodexOAuthPending`**, **`authorize_url`**, **`callback_port`**) is published from **`tddy-service`** / **`tddy-coder`** via **`codex_oauth_scan`** and related wiring. On the **operator** machine, **`tddy-daemon`** (**`oauth_loopback_tunnel`**) binds loopback TCP and sends raw HTTP bytes over **`LoopbackTunnelService.StreamBytes`** to the session host; **tddy-desktop** does not **`Bun.listen`** for OAuth in production when the embedded or standalone daemon runs with **`livekit.common_room`**. The **`codex-acp`** backend may surface the same authorize URL file via **`codex login`** when ACP session setup fails with auth-like errors; see [Codex ACP backend](../coder/codex-acp-backend.md).

## Operator loopback tunnel (`tddy-daemon`)

With **`livekit.common_room`** set, the daemon’s common-room **`Room`** drives **`run_oauth_tunnel_supervisor`**: scan **`daemon-*`** participants for pending **`codex_oauth`** metadata, open **`authorize_url`** in the system browser, listen on **`127.0.0.1:{callback_port}`**, and bridge TCP to **`loopback_tunnel.LoopbackTunnelService`** on the chosen session participant. **`codex_oauth_participant_metadata`** parses the same JSON shape as the desktop **`codex-oauth-metadata`** helper. Package reference: **[`oauth-loopback-tunnel.md`](../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)**; room lifecycle ties to **[livekit-peer-discovery.md](livekit-peer-discovery.md)**.

## Validation

- **Scheme**: **`https`** only; HTTP authorize URLs fail with **`SchemeNotHttps`**.
- **Host**: Must match **`CodexOAuthHostAllowlist`** (default hosts include **`auth.openai.com`**, **`openai.com`**, **`chatgpt.com`**; matching is case-insensitive on the host segment).
- **Session**: When **`active_session_id`** is **`Some`**, it must equal **`session_correlation_id`** or **`CorrelationMismatch`** is returned.

## Capture and relay

- **`dispatch_browser_open_capture`**: Scans argv for the first parseable **`https`** URL; if none is present, **`NoHttpsAuthorizeUrlInBrowserArgv`** applies.
- **`relay_oauth_callback_to_registered_listener`**: Fills a **`HashMap`** from **`callback_url`** query pairs (for example **`code`**, **`state`**); no network I/O inside this function.

Logging uses **`log::`** with target **`tddy_daemon::codex_oauth`**; full query strings and tokens are omitted from log lines.

## Technical reference

- **[`codex-oauth-relay.md`](../../../packages/tddy-daemon/docs/codex-oauth-relay.md)** — API, tests, and commands
- **[`oauth-loopback-tunnel.md`](../../../packages/tddy-daemon/docs/oauth-loopback-tunnel.md)** — operator TCP + LiveKit tunnel supervisor
- **[Codex OAuth web relay (product)](../web/codex-oauth-web-relay.md)** — dashboard UX and scope

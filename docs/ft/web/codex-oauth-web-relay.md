# Codex OAuth web relay

## Purpose

Operators run **tddy-coder** with the **OpenAI Codex** backends (**`--agent codex`** or **`--agent codex-acp`**) in environments without a usable desktop browser (remote agents, headless hosts, CI-like shells). The **`codex`** CLI OAuth path (including the helper used when **`codex-acp`** signals auth-like failures) invokes the platform browser opener (for example via **`BROWSER`**). See [Codex ACP backend](../coder/codex-acp-backend.md). This product area covers surfacing the **HTTPS authorize URL** inside **tddy-web** and relaying the **OAuth callback** (authorization code and state) back to the Codex process so login completes with the same semantics as a local browser session.

## Web dashboard

**tddy-web** provides **`CodexOAuthDialog`**: a modal with a dismiss control, an embedded **iframe** for the authorize step when framing is permitted, and a **documented non-iframe path** when the identity provider blocks embedding (**`X-Frame-Options`** / CSP **`frame-ancestors`**): explanatory copy plus an **Open authorization in new window** link (**`target="_blank"`**, **`rel="noopener noreferrer"`**).

Component tests under **`packages/tddy-web/cypress/component/`** cover dialog visibility, dismiss unmounting, and the embedding-blocked panel.

## Daemon library

**`tddy-daemon`** exposes **`codex_oauth_relay`**: pure validation and parsing helpers used by future RPC and hook wiring.

- **`validate_codex_oauth_authorize_url`**: requires **`https`**, host membership in **`CodexOAuthHostAllowlist`**, and matching **session correlation** when an active session id is supplied.
- **`dispatch_browser_open_capture`**: accepts argv as produced by a **`BROWSER`** wrapper, selects the first **`https`** URL, validates it for the given session id, and returns **`CodexOAuthAuthorizeCapture`** (session id + authorize URL string).
- **`relay_oauth_callback_to_registered_listener`**: parses the callback **`Url`** query into **`CodexOAuthCallbackDelivery`** (session id + query map) for layers that forward to the Codex loopback listener.

Structured logs use the **`tddy_daemon::codex_oauth`** target; authorize URLs and codes are not logged in full.

## Relationship to the full flow

End-to-end capture requires **`BROWSER`** (or equivalent) routing to Tddy, daemon **RPC or event delivery** to the correct **tddy-web** session, and **HTTP or IPC** delivery of the callback to the running Codex CLI listener. Those integrations live outside this document; see **[Codex OAuth relay (daemon)](../daemon/codex-oauth-relay.md)** and **[`codex-oauth-relay.md`](../../../packages/tddy-daemon/docs/codex-oauth-relay.md)** for technical detail.

## Related documentation

- **[Local web development](local-web-dev.md)** — daemon + Vite proxy for **`/rpc`**
- **[Coder overview](../coder/1-OVERVIEW.md)** — backend selection
- **[Codex ACP backend](../coder/codex-acp-backend.md)** — **`codex-acp`** and OAuth retry
- **[`codex-oauth-dialog.md`](../../../packages/tddy-web/docs/codex-oauth-dialog.md)** — tddy-web component reference

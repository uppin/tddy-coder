# Tddy desktop app (Electrobun) — design

## Purpose

Ship a **small native desktop shell** (`packages/tddy-desktop`) that:

1. **Embeds or serves `tddy-web`** locally so operators get a first-class app window instead of juggling browser tabs and Vite/daemon ports.
2. **Accepts the Codex OAuth browser callback on operator loopback** (`http://127.0.0.1:<port>/auth/callback` or Codex’s chosen port) and **forwards raw HTTP bytes** over **LiveKit** using **`loopback_tunnel.LoopbackTunnelService.StreamBytes`**, so the **session host** dials **`127.0.0.1:<port>`** and Codex sees the same callback as a purely local run.
3. **Uses the existing LiveKit room** (same model as the web terminal): **`tddy-coder`** publishes **`codex_oauth` participant metadata** (pending, authorize URL, callback port); the desktop app opens the browser and runs **`installLiveKitOAuthRelay`** with an injected **`startOAuthTcpTunnel`** implementation that pipes each accepted TCP connection through the bidi tunnel.

This document is the **WHAT**; implementation lives in `packages/tddy-desktop` and incremental changes in `tddy-web`, `tddy-livekit`, and `tddy-coder` as needed.

## Non-goals (initial phases)

- Replacing the in-browser dashboard for all users (desktop is **optional**).
- Bundling **`tddy-coder`** inside the app (it remains a separate agent process).
- Windows or Linux desktop bundles for **`tddy-daemon`** (macOS is the first bundled target).
- Storing long-lived OpenAI tokens in the desktop app (credentials stay where Codex/`tddy-coder` already persist them).

## Actors

| Actor | Role |
|--------|------|
| **Tddy Desktop** | Electrobun **main process** (Bun): window management, optional local static server, **OAuth loopback TCP accept** (injected tunnel), LiveKit **Connect** client for **`StreamBytes`**, optional **embedded `tddy-daemon`** spawn on macOS. |
| **tddy-web UI** | Same React app as today; loaded from `file://` bundle, embedded dev server, or proxied `https://` in webview. |
| **LiveKit room** | Shared **presence / RPC** room already used for terminal and participant list (e.g. `tddy-lobby` + session-scoped identities). |
| **tddy-coder** (child) | Publishes **`codex_oauth` metadata** (`pending`, `authorize_url`); runs **Codex** / **codex-acp** which listens on loopback for OAuth callback **on the agent host**. |
| **OpenAI / Codex OAuth** | Browser navigates to `https://auth.openai.com/...`; redirect URI is **fixed by Codex** (typically `http://127.0.0.1:<ephemeral>/auth/callback` on the **machine running Codex**). |

## Problem the desktop app solves

- **Remote agent host**: Codex binds OAuth callback on **its** loopback. A developer’s laptop browser cannot hit that address. Today the mitigations are **SSH `-L`**, **device code**, or **copying `auth.json`** ([Codex auth](https://developers.openai.com/codex/auth/)).
- **UX**: Even locally, a dedicated window + deep links improves discoverability vs “open Vite URL + daemon port”.

The desktop app targets **relay**: laptop runs **desktop + browser**; **callback hits the laptop**; **callback payload is delivered to `tddy-coder` over LiveKit** so Codex on the remote host can complete login (see *Relay variants* below).

## High-level architecture

```mermaid
flowchart LR
  subgraph desktop["Tddy Desktop (Electrobun)"]
    MP[Main process Bun]
    WV[Webview tddy-web]
    TCP[Loopback TCP accept]
    LK_C[LiveKit Connect client]
    MP --> WV
    MP --> TCP
    MP --> LK_C
  end
  subgraph cloud["LiveKit SFU"]
    ROOM[Room]
  end
  subgraph agent["Agent host"]
    TC[tddy-coder child]
    CX[Codex / codex-acp]
    LK_S[LiveKit participant]
    BR[LoopbackTunnel bridge]
    TC --> CX
    TC --> LK_S
    LK_S --> BR
    BR -->|127.0.0.1:port| CX
  end
  WV -->|HTTPS RPC same as today| DMN[tddy-daemon / Vite proxy]
  LK_S <-->|data channel RPC| ROOM
  LK_C <-->|StreamBytes TunnelChunk| ROOM
  TCP -->|raw HTTP bytes| LK_C
```

## LiveKit: OAuth metadata and loopback tunnel

- **`tddy-coder`** publishes **`codex_oauth` JSON** on the session participant metadata channel (`pending`, **`authorize_url`**, **`callback_port`**, **`state`**, etc.). **`tddy-web`** **ParticipantList** and the desktop app consume the same shape.
- **Session host** registers **`loopback_tunnel.LoopbackTunnelService`** on the LiveKit **tddy-rpc** surface alongside **TerminalService** (and **TokenService** when API key mode is used). **`StreamBytes`** is a bidi stream of **`TunnelChunk`**: the **first** chunk sets **`open_port`** (the Codex loopback port) and may carry initial payload; later chunks carry upstream or downstream bytes. The server connects to **`127.0.0.1:{open_port}`** and refuses **`open_port < 1024`**.
- **Desktop** calls **`installLiveKitOAuthRelay`** with **`startOAuthTcpTunnel`** supplied by the host (production wiring uses the Connect transport and **`loopback_tunnel_pb`**; tests inject a fake tunnel). The desktop process does **not** parse OAuth query parameters for the production path; **HTTP semantics stay on the session host** where Codex listens.
- **`tddy_daemon::codex_oauth_relay`** remains the shared validation/parsing library for **authorize URLs** and callback **URLs** where those layers apply; tunnel mode is **byte-transparent** between browser TCP and Codex loopback.

## OAuth port negotiation

Codex picks an **ephemeral port** (e.g. 1455). The desktop app must learn it:

- **Preferred:** extend metadata JSON to include `callback_origin` / `callback_port` when available from Codex stderr or a small sidecar file written by `tddy-coder` (same session dir as `codex_oauth_authorize.url`).
- **Fallback:** desktop listens on a **fixed** local port and user configures Codex/`~/.codex/config.toml` if upstream supports **`mcp_oauth_callback_url`** or future **ChatGPT login** callback override (verify per Codex version).

## `packages/tddy-desktop` layout

```
packages/tddy-desktop/
  README.md                 # Dev quickstart; embedded daemon env
  electrobun.config.ts      # build.copy includes resources/bin/tddy-daemon
  src/bun/
    index.ts                # BrowserWindow, optional daemon spawn, relay wiring
    livekit-oauth-relay.ts  # Metadata watch + installLiveKitOAuthRelay (injected tunnel)
    embedded-daemon.ts      # Resolve config/binary paths; spawn tddy-daemon (macOS)
  resources/bin/            # Release binary from prebuild (gitignored)
```

## Electrobun specifics

- **Main process:** Bun + Electrobun APIs for windows and webviews ([Electrobun docs](https://electrobun.dev/docs/)).
- **Renderer:** load **`tddy-web`** build output or **`VITE_URL`** in dev via allowed navigation / devtools policy.
- **Updates:** out of scope for v0; later consider Electrobun’s small delta updates.

## Security

- **Callback traffic** contains **authorization codes** — treat as secret in transit:
  - tunnel **raw TCP** (HTTP bytes) over the **LiveKit data channel** RPC already authenticated by **room JWT**;
  - restrict **destination identities** (only the daemon participant for the session);
  - **never** log full HTTP requests or query strings.
- **Metadata** from LiveKit is **not** trusted for code execution; only **HTTPS** authorize URLs (existing web parser rule).
- **Deep links** (`tddy://…`) optional later; must validate session id.

## Phases

1. **Shell** (implemented): Electrobun app loads **production `tddy-web` dist** from disk or env URL; Connect flow unchanged (RPC via daemon as today).
2. **OAuth discovery** (implemented): Desktop reads **`codex_oauth` metadata**; opens browser; listens on **`127.0.0.1:{callback_port}`** for the browser callback.
3. **Relay MVP** (implemented): **`LoopbackTunnelService.StreamBytes`** (bidirectional) pipes TCP bytes from the operator machine to **`127.0.0.1:{port}`** on the session host where Codex’s loopback listener receives the same HTTP `GET /auth/callback` as a local run.
4. **Embedded daemon (macOS)** (implemented): see *Bundled `tddy-daemon` (macOS)* below.
5. **Polish:** Installer, code signing, auto-update, tray icon.

## Bundled `tddy-daemon` (macOS)

The desktop main process may **spawn `tddy-daemon`** so the webview reaches **`/api/config`** and Connect-RPC without a separate terminal. This targets **macOS** first; other desktop OS bundles are out of scope for now.

- **Config:** Daemon loads YAML via **`--config` / `TDDY_DAEMON_CONFIG`** (existing daemon behavior). The app resolves a config path from **`TDDY_DAEMON_CONFIG`**, repo-root **`dev.desktop.yaml`** when unset in dev, and root **`.env`** loading consistent with **`./web-dev`**.
- **Binary resolution (in order):** **`TDDY_DAEMON_BINARY`**, **`resources/bin/tddy-daemon`** (populated by **`prebuild`** / **`build-daemon`**), then workspace **`target/release/tddy-daemon`** or **`target/debug/tddy-daemon`** at the repo root. **`prebuild`** runs **`cargo build --release -p tddy-daemon`** and copies into **`resources/bin/`**; **`electrobun.config.ts`** **`build.copy`** ships the binary inside the app bundle.
- **Lifecycle:** Start when the main process starts (when config and binary resolve); **SIGTERM** / process exit tears down the child so the daemon listen port is released in normal quit paths.
- **Security:** No API keys are embedded in the app; the user YAML remains the trust boundary. A desktop-spawned daemon is a **dev convenience** and runs as the current user—production installs may still use **launchd** or another supervisor with different privileges.

## Related docs

- [Codex OAuth web relay](../web/codex-oauth-web-relay.md)
- [Codex OAuth relay (daemon)](../daemon/codex-oauth-relay.md)
- [Local web development](../web/local-web-dev.md)
- [Web terminal / LiveKit](../web/web-terminal.md)

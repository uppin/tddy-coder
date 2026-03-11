# Dioxus Web Terminal (Archived)

**Status:** Archived. Implementation removed. Use the official Ghostty-web demo instead: `npx @ghostty-web/demo@next` → http://localhost:8080.

This document preserves the architecture and implementation details of the Dioxus-based web terminal for future reference.

## Overview

A web application that displays an interactive terminal in the browser using ghostty-web. Built with Dioxus 0.7 fullstack: single Rust codebase, WASM client, Axum server, PTY + WebSocket backend.

## Architecture

| Layer | Technology |
|-------|------------|
| Frontend | Dioxus 0.7 (Rust → WASM), ghostty-web via JS interop |
| Backend | Dioxus fullstack (Axum), PTY via portable-pty |
| Terminal | ghostty-web npm package (WASM VT100 parser, xterm.js-compatible API) |
| Communication | WebSocket `/ws/terminal` between browser and server |

## Key Components

### 1. ghostty-bridge.js

Bootstrap script loaded from `public/`. Injects a module script that:

- Loads ghostty-web from `/ghostty-web/ghostty-web.js` (copied from npm `ghostty-web` via postinstall)
- Calls `init()` then exposes `{ Terminal, FitAddon, initTerminal }` on `window.__ghostty`
- Dispatches `ghostty-ready` event when loaded
- `initTerminal(container, wsUrl)` creates WebSocket, Terminal, FitAddon, ResizeObserver; wires PTY I/O; handles resize via `\x1b]resize;{cols};{rows}\x07`

### 2. Dioxus App (app.rs)

- Renders a `[data-terminal]` div
- On mount, injects JS that waits for `ghostty-ready`, then calls `window.__ghostty.initTerminal(el, wsUrl)`
- `ws_url()` built from `window.location` (ws/wss based on protocol)

### 3. WebSocket Handler (ws.rs)

- Route: `GET /ws/terminal`
- On connect: spawn PTY via `pty::create_session(None)`
- PTY → WebSocket: blocking read in `spawn_blocking`, forward via `mpsc` to WebSocket
- WebSocket → PTY: parse `\x1b]resize;{cols};{rows}\x07`, call `pty::resize_session`; otherwise write raw bytes to PTY

### 4. PTY (pty.rs)

- `portable_pty::native_pty_system()`, `openpty`, `spawn_command`
- Shell: `$SHELL` or `/bin/sh`
- `TERM=xterm-256color`
- Resize via `master.resize(PtySize { rows, cols, ... })`

### 5. Entry Point (main.rs)

- `#[cfg(feature = "server")]`: `dioxus::serve` with router, route `/ws/terminal` to `ws::terminal_ws_handler`
- `#[cfg(not(feature = "server"))]`: `dioxus::launch(app::App)` for client-only

## Package Structure

```
packages/tddy-web/
├── Cargo.toml          # features: web, server
├── Dioxus.toml         # asset_dir = public, script = ghostty-bridge.js
├── package.json        # ghostty-web dep, postinstall: cp to public/ghostty-web
├── serve.sh            # nix develop + wasm-bindgen-cli 0.2.114 + dx serve
├── public/
│   ├── ghostty-bridge.js
│   └── ghostty-web/   # from npm postinstall
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── terminal.rs    # ws_url(), mount_terminal (optional)
│   ├── pty.rs
│   └── ws.rs
└── cypress/
    ├── cypress.config.js
    ├── support/e2e.js
    └── e2e/terminal.cy.js
```

## Dependencies

```toml
dioxus = { version = "0.7", features = ["fullstack"] }
axum = { version = "0.8", features = ["ws", "json"] }
portable-pty = "0.9"
wasm-bindgen, js-sys, web-sys  # client
```

## Usage (Before Removal)

```bash
cd packages/tddy-web && npm install
./server.sh   # from repo root
# or: cd packages/tddy-web && ./serve.sh
```

Visit http://localhost:8080.

## Recommended Alternative

Use the official Ghostty-web demo:

```bash
./serve_term.sh
# or: npx @ghostty-web/demo@next
```

Serves at http://localhost:8080 with WebSocket PTY at `/ws`. No custom build required.

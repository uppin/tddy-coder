# PRD: Bundle tddy-daemon with Tddy Desktop (macOS)

## Summary

Ship a **`tddy-daemon`** binary with **`packages/tddy-desktop`** (Electrobun) and **start it automatically** when the desktop app launches, using the user’s **existing daemon YAML** (no bundled secrets). LiveKit and listen settings remain defined in that YAML; the desktop process only spawns the daemon with a resolved config path.

## Background

Today the desktop webview expects **`/api/config`** and Connect-RPC on the daemon HTTP port (typically **8899**). Developers must run **`tddy-daemon`** separately. Packaging the daemon with the desktop removes that manual step for local use on **macOS** (first target).

## Requirements

1. **macOS first** — build/packaging and spawn logic validated on macOS; other OSes out of scope for this PRD.
2. **Config source** — daemon loads YAML via **`--config`** / **`TDDY_DAEMON_CONFIG`** (existing behavior). Desktop resolves config path from env and/or documented default search order (e.g. explicit env required for v1 to avoid wrong file).
3. **Lifecycle** — start daemon after (or before) BrowserWindow as needed; **stop** daemon child when the desktop app exits (normal quit and crash where possible).
4. **Binary origin** — release **`tddy-daemon`** artifact produced in CI or `electrobun build` prerequisite (`cargo build --release -p tddy-daemon`), copied into an app resource path discoverable at runtime.
5. **Failure UX** — if config missing, binary missing, or daemon exits: log clearly; optional UI toast or console message (minimal v1: stderr + optional Electrobun API if available).
6. **Security** — do not embed API keys; user YAML remains the trust boundary. Document that production daemon often runs as root; **desktop-spawned** daemon is a **dev convenience** and may run as the current user—call out permission/spawn constraints in docs.

## Non-goals (this PRD)

- Windows/Linux bundles.
- Replacing system-wide or launchd-managed daemon installs.
- First-run config wizard (user selected “user YAML” only).

## Affected areas

- **`packages/tddy-desktop`** — main process spawn, resource layout, `electrobun` build.
- **`packages/tddy-daemon`** — only if CLI or path ergonomics need a small change (prefer env-only).
- **Root / CI** — optional script to build and copy `tddy-daemon` into desktop resources.
- **Docs** — `packages/tddy-desktop/README.md`, feature doc under `docs/ft/desktop/` if needed.

## Success criteria

- With valid **`TDDY_DAEMON_CONFIG`** (or agreed default) and a built app, **one launch** of desktop starts **both** Vite-backed UI (existing `desktop:dev` flow) **or** bundled web **and** listening daemon; **`/api/config`** is available at the port from YAML.
- Quitting the desktop **terminates** the child daemon (no orphan listener on 8899 in normal cases).
- **`bun test`** / package tests for desktop cover spawn contract (mock or integration as appropriate).

## References

- `packages/tddy-daemon/src/main.rs` (`--config` required).
- `packages/tddy-desktop/src/bun/index.ts` (BrowserWindow + optional LiveKit OAuth relay).

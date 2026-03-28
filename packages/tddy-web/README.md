# tddy-web

React dashboard and terminal UI for **tddy**: Connection Screen (daemon mode), **Ghostty** terminal over LiveKit, and Cypress/Storybook tooling.

## Quick Start

### Development

```bash
# From repo root (nix dev shell)
./dev bun install
./dev bash -c 'cd packages/tddy-web && bun run storybook'
```

### Testing

```bash
./dev bash -c 'cd packages/tddy-web && bun test'
./dev bash -c 'bun run --cwd packages/tddy-livekit-web build && cd packages/tddy-web && bun run cypress:component'
```

### Build

```bash
./dev bash -c 'cd packages/tddy-web && bun run build'
```

## Architecture

The app uses **Vite**, **React**, **Connect-RPC** to the daemon **`/rpc`** routes, and **`tddy-livekit-web`** for LiveKit terminal streaming. Authentication uses GitHub via the shared auth flow; the Connection Screen lists projects and sessions from the daemon.

## Documentation

### Product (what)

- [Web terminal — feature overview](../../docs/ft/web/web-terminal.md)
- [Local web dev](../../docs/ft/web/local-web-dev.md)

### Technical (how)

- [Terminal overlay (LiveKit)](./docs/terminal-overlay.md)
- [Changesets](./docs/changesets.md)

## Related packages

- [tddy-livekit-web](../tddy-livekit-web/README.md) — LiveKit Connect-RPC transport and terminal service
- [tddy-daemon](../tddy-daemon/README.md) — Connection and session APIs

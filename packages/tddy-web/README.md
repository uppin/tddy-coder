# tddy-web

React dashboard and browser terminal (**Ghostty** + LiveKit) for session workflows served by **tddy-daemon**.

## Documentation

- **Web terminal (product)**: [docs/ft/web/web-terminal.md](../../docs/ft/web/web-terminal.md)
- **Web changelog**: [docs/ft/web/changelog.md](../../docs/ft/web/changelog.md)
- **Local development**: [docs/ft/web/local-web-dev.md](../../docs/ft/web/local-web-dev.md)

## Development

From the repository root (with the dev shell):

```bash
bun install
./dev bash -c 'cd packages/tddy-web && bun run build'
./dev bash -c 'cd packages/tddy-web && bun test'
./dev bash -c 'cd packages/tddy-web && bun run cypress:component'
```

See [AGENTS.md](../../AGENTS.md) for the full toolchain.

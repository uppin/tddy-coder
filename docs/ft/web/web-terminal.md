# Web Terminal

## Summary

A web application that displays an interactive terminal in the browser using the ghostty-web terminal emulator. The app serves as the foundation for future streaming of the tddy-coder TUI, but starts as a generic terminal that spawns the user's default shell.

## Recommended Setup

Use the official Ghostty-web demo:

```bash
./serve_term.sh
# or: npx @ghostty-web/demo@next
```

Serves at http://localhost:8080 with WebSocket PTY at `/ws`. Works best on Linux and macOS.

## Background

tddy-coder currently operates as a CLI/TUI application. To enable remote observation and collaboration, a web-based terminal viewer is needed. A previously implemented Dioxus fullstack solution was archived; its architecture is documented in [docs/kb/dioxus-web-terminal.md](../../kb/dioxus-web-terminal.md).

## Architecture (Reference)

- **Terminal emulator**: ghostty-web npm package (WASM-compiled Ghostty VT100 parser, xterm.js-compatible API)
- **PTY**: Server-side PTY spawning the user's default `$SHELL`
- **Communication**: WebSocket between browser (ghostty-web Terminal) and server (PTY process)

## Future Scope (Not in this version)

- Streaming tddy-coder TUI instead of a generic shell
- Multi-session support
- Authentication and access control
- Terminal resize synchronization
- Session persistence and reconnection

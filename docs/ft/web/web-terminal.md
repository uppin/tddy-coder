# Web Terminal

## Summary

A web application that displays an interactive terminal in the browser using the ghostty-web terminal emulator. **tddy-demo TUI streaming over LiveKit** is implemented: GhosttyTerminal component in tddy-web receives ANSI bytes via TerminalService RPC, with Cypress E2E validation. A standalone generic terminal (user's default shell over WebSocket) remains available via the Ghostty-web demo.

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

## Connected Terminal UX

When the terminal connects and renders, it supports:

- **Fullscreen**: Fills 100% of the viewport (width and height). Overlay buttons: Disconnect and Ctrl+C.
- **Auto-focus**: Keyboard focus is set on the terminal when ready. User can type immediately. (On mobile, auto-focus is disabled; see Mobile UX.)
- **Adaptive size**: FitAddon auto-sizes the terminal to its container. Resize events are sent to the virtual TUI via `\x1b]resize;{cols};{rows}\x07`.
- **Touch/mouse mode**: When `--mouse` is set on tddy-coder, the TUI sends EnableMouseCapture. GhosttyTerminal encodes SGR mouse sequences `\x1b[<Pb;Px;PyM/m` (press/release) and forwards them via onData. Click-to-select and scroll work. Touch events (touchstart/touchend) are forwarded for tap-to-click on mobile.

### Mobile UX

On touch-capable devices or narrow viewports (width &lt; 768px):

- **Keyboard-aware resize**: The terminal container uses the Visual Viewport API. When the virtual keyboard opens, the container shrinks to fit the visible area above the keyboard; when it closes, the terminal fills the screen again.
- **Manual keyboard button**: A floating "Keyboard" button appears at the bottom center. Tapping it focuses the terminal (opens the virtual keyboard). The button hides while the keyboard is open and reappears when it closes.
- **Touch forwarding**: Tap-to-click works for TUI menus and interactive elements via SGR mouse sequences.

## Future Scope

- Multi-session support
- Authentication and access control
- Session persistence and reconnection

# TUI E2E Testing

**Status:** Deferred  
**Related:** `packages/tddy-tui`

## Context

The TUI (`tddy-tui`) currently has no end-to-end tests. Unit tests cover individual components, but there's no way to assert on rendered screen output or simulate user keyboard interaction in an automated test.

## Approaches

All serious TUI E2E testing works by spawning the app inside a **pseudo-terminal (PTY)**, reading the virtual screen buffer, and injecting keystrokes.

### Rust-specific options

| Tool | Version | Notes |
|------|---------|-------|
| **ratatui-testlib** | v0.1.0 (Dec 2025) | First-party ratatui integration testing. PTY-based, `insta` snapshot support, keyboard simulation, async/Tokio, headless CI. [crates.io](https://crates.io/crates/ratatui-testlib) |
| **termwright** | v0.2.0 (Feb 2026) | Playwright-like terminal automation. Framework-agnostic, screen reading (text/colors/cursor), wait conditions (text/regex/stability), input simulation, PNG screenshots, box detection. [GitHub](https://github.com/fcoury/termwright) |
| **ptytest** | v0.1.0 | Lightweight PTY screen-compare testing using `vt100` emulation. [GitHub](https://github.com/da-x/ptytest) |
| **TestBackend** (ratatui built-in) | — | In-memory buffer rendering for widget/layout unit tests. Not real terminal behavior. Pairs with `insta` for snapshot regression. [Recipe](https://ratatui.rs/recipes/testing/snapshots/) |

### Cross-language options

| Tool | Language | Notes |
|------|----------|-------|
| **tui-test** (Microsoft) | TypeScript | E2E framework for CLI/TUI, PTY-based |
| **curtaincall** | Python (pytest) | PTY + VT100 emulation, auto-waiting locators, color assertions |
| **pytest-tmux** | Python (pytest) | Real tmux sessions, screen/row assertions with retries |

## Recommendation

For `tddy-tui` (ratatui-based):

1. **ratatui-testlib** — tightest integration, `insta` snapshots, purpose-built
2. **termwright** — richer automation API (Playwright-style), framework-agnostic, useful if tests need to drive the full binary

## Common patterns

- **Screen assertions**: read virtual terminal buffer, match via substring/regex/full snapshot
- **Input injection**: send raw bytes or structured key events into PTY
- **Wait/poll**: auto-wait for text appearance, screen stability, or process exit
- **Snapshot testing**: capture full screen as string, compare against stored snapshots (e.g. `insta`)

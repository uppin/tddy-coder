# 2026-11-02 — OSC 52 copy: clipboard reachability in Tddy Desktop and insecure origins is unverified

**Category:** Deferred from `session-tabs` (#807)
**Source:** `docs/dev/1-WIP/2026-11-02-session-tabs-osc52-clipboard.md` (wrapped), `#session-tabs 4/7`

The OSC 52 tap calls `copyToClipboard` (`navigator.clipboard.writeText`, with an `execCommand("copy")` fallback). The Cypress specs prove that call is made with the decoded text. Nobody has checked that it **lands on the clipboard** in a real browser, in the Tauri webview of Tddy Desktop, or on a plain-http LAN origin (see `packages/tddy-web/docs/insecure-origin-constraints.md`), where the async clipboard API may refuse a write that is not tied to a user gesture.

**Why deferred:** if the Tauri webview refuses, the fix is a clipboard plugin in `tddy-desktop`, which is a new dependency and needs the developer's decision first.

**Recommendation:** try tmux copy-mode in a tmux tab in the installed desktop app and in a browser over https. Only if it fails, add the Tauri clipboard plugin (a new dependency, so ask first).

**Defer consent:** the developer, 2026-11-03 (`#session-tabs` sweep gate): deferred as recorded; verify tmux copy-mode in the installed desktop app and over https later, add the Tauri clipboard plugin only after asking.

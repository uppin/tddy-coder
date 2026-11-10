# 2026-11-08 — ghostty-web is removed and the terminal components are renamed (#xterm 4/4, PR #869)

**Type:** Refactor

`ghostty-web` is gone from every `package.json` and `bun.lock`, along with `ghosttyEngine`,
`lib/ghosttyInstance.ts`, `lib/terminalDprWatch.ts` (xterm.js handles the device pixel ratio), the
`scripts/vite-prebundle.ts` entry, Tauri's `'wasm-unsafe-eval'` CSP source and the storybook render
test's terminal-story exclusion. `GhosttyTerminal` → `TerminalView`, `GhosttyTerminalSession` →
`TerminalSession` (with their `Props`/`Handle`/`Theme` types), testIds `ghostty-terminal*` →
`terminal*`, story ids `components-ghosttyterminal--*` → `components-terminal--*`,
`cy.visitGhosttyStory` → `cy.visitTerminalStory`, and the spec, driver and e2e files follow.
`scripts/no-ghostty.test.ts` (CI: `web.yml` "Web rule tests") keeps `ghostty` out of the web
packages outside change history. Product docs (`docs/ft/web/`) and `docs/dev/guides/ci.md` describe
xterm.js. Rust e2e test names are left for
[docs/dev/todo/2026-11-08-rename-rust-ghostty-virtual-terminal-e2e-tests.md](../todo/2026-11-08-rename-rust-ghostty-virtual-terminal-e2e-tests.md).
Packages: tddy-terminal-web, tddy-session-web, tddy-storybook, tddy-web, tddy-desktop,
tddy-client-web, tddy-livekit-web. See
[tddy-terminal-web](../../../packages/tddy-terminal-web/docs/changesets/2026-11-08-drop-ghostty.md).

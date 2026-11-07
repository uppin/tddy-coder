# 2026-11-07 — UI kit package (#web-split 3/16, PR #840)

**Type:** Architecture Change

New workspace package `tddy-ui-web`, moved out of `tddy-web` with history: the shadcn primitives
(`button`, `inline-error`, `scroll-area`, `select`, `table`, `toast`, `tooltip`, flattened to
`src/<name>.tsx`), `cn` (`src/cn.ts`, was `lib/utils`), `useIsMobile` (with its `bun test`),
`useVisualViewport`, and the theme (`src/theme.css`, exported as `tddy-ui-web/theme.css`, with
tailwind `@source` for `./`, `../../tddy-web/src` and `../../tddy-*-web/src`). Exports map to `src/`.

- Own Vite/Cypress config, React Compiler switch identical to `tddy-web`'s
  (`CYPRESS_DISABLE_REACT_COMPILER=1`). Specs `Button`, `Toast`, `useVisualViewport` moved.
- Boundary rule `src/packageBoundary.test.ts` on the shared checker
  `scripts/web-package-boundary.ts` (`forbiddenImports(packageDir, rules)`): no `tddy-web`, generated
  code, `@connectrpc/*`, `livekit-client`.
- PR check `web / tddy-ui-web tests` (`.github/workflows/web-tddy-ui-web.yml`).
- No shared pure helper moved: none has a consumer outside `tddy-web` yet. Within-app consumer
  counts (participantRole 6, debugMask 6, testId 4, toolShortcuts/terminalHistoryLoader/shellQuote/
  presenceIdentity 3) — each moves with the first package that consumes it from outside the app.

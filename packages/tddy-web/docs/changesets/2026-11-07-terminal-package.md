# 2026-11-07 — The terminal comes from `tddy-terminal-web` (#web-split 8/17, PR #845)

- The terminal modules, chrome, feed, input delivery and helpers moved to `tddy-terminal-web` (its
  changeset); sessions, chat, PR-stack and `StandaloneConnectionScreen` import them from the package.
- `sessionRuntimeRegistry` re-exports the package's `ByteDelta` instead of defining its own.
- The `terminal-*.md` docs moved to the package; `README.md` links there.

# 2026-07-04 — **`seed_claude_credentials`/`seed_claude_local_install`

**Type:** Feature

persistent jail-home seeding** — relocated from the `tddy-sandbox-app` binary into `claude_cli` (and re-exported; `seed_claude_local_install` is unix-only) so the daemon and the `./claude-sandbox` app share them: seed `.claude/.credentials.json` once **without overwriting** a jail-refreshed token, and mirror the claude install (`~/.local/bin/claude` + versioned symlink) into a persistent jail `$HOME` so the in-jail startup self-check passes. Cross-package: [docs/dev/changesets/](../../../../docs/dev/changesets/). PR [#281](https://github.com/uppin/tddy-coder/pull/281). (tddy-sandbox-recipes, tddy-daemon, tddy-sandbox-app)

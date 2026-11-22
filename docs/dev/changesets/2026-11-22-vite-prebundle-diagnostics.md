# 2026-11-22 — Cypress component runs name the dependency that escapes the Vite pre-bundle (PR #1001)

**Type:** CI diagnostics

Motivating case: PR #997's web shard 2 (commit 0a4cd63e2) lost `SessionsDrawerCrossHostAcceptance.cy.tsx`
at spec load — "Failed to fetch dynamically imported module …/cypress/support/component.ts", 0 tests
run — the signature of Vite discovering a dependency mid-run, re-optimising and reloading. Nothing in
the shard log said which dependency, so the flake was left unfixed and made diagnosable instead.

`scripts/vite-prebundle.ts` adds `prebundleDiagnostics()`, a serve-only Vite plugin every web
package's `vite.config.ts` installs next to `prebundledDeps()`. It wraps the client dependency
optimiser's `registerMissingImport` to name each dependency the up-front pre-bundle missed, names them
again on the re-optimise reload, and copies Vite's optimiser warnings and errors (a failed scan with
its cause), all as `[vite-prebundle] <package>: …` lines on stderr — the plugin's own writes, since
Cypress keeps the dev server's logger output out of the job log. `web.yml` sets
`VITE_PREBUNDLE_LOG_DIR`, so each web shard also keeps the lines in
`reports/vite-prebundle/<package>.log`, uploaded as `vite-prebundle-web-shard-i`. It only observes;
no run fails because of it. On a clean run it prints one `watching the dependency optimiser` line per
dev server.

`scripts/vite-prebundle.test.ts` gains the plugin's tests and joins `ci.yml`'s "CI script tests"
step, which it was never part of.

How to read a failure: [ci.md § Web tests](../guides/ci.md) ("When a dependency still escapes the
pre-bundle").

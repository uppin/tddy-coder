# 2026-11-07 — Per-package web PR checks (#web-split 1/16, PR #838)

**Type:** Architecture change (CI)

The single `Web tests` job in `ci.yml` is replaced by one workflow per JS workspace package
(`.github/workflows/web-<package>.yml`), each calling the reusable `web-package.yml`:
build gate → `changes` (`scripts/web-affected.ts`) → `<package> tests`, shown and required as
**`web / <package> tests`**. A package runs only when a PR touches it, a workspace package it depends
on (transitively), its caller workflow, or a global input (root `package.json`, `bun.lock`,
`bunfig.toml`, `flake.*`, `build-gate.yml`, `web-package.yml`, `scripts/web-affected.ts`).
Behaviour is documented in [ci.md § Web package checks](../guides/ci.md#web-package-checks).

- `scripts/web-affected.ts` + `web-affected.test.ts`; `scripts/web-workflows.test.ts` pairs every
  workspace package with exactly one caller.
- Callers set `needs-web-bundle`, `needs-tddy-coder`, `needs-rust-fixture`, `needs-livekit-image`,
  `has-cypress`. Web checks no longer wait on `rust-build`; they build the binaries they exec.
- A failed `changes` job fails the check instead of skipping it (a skip reports success).
- `test:ci` added to tddy-web, tddy-livekit-web (44 bun tests no script ran before) and
  tddy-rust-typescript-tests (first time in CI).
- The `Generated code` job now runs every `scripts/*.test.ts` (closes
  `2026-10-05-ci-script-tests-are-not-run-in-ci`), with `--timeout 30000`; two cold-start-sensitive
  tests got explicit timeouts.
- At merge: the `master` ruleset swaps `Web tests` for the seven `web / <package> tests` contexts.

**Deferred:** [desktop check does not follow the tddy-web bundle](../todo/2026-11-07-desktop-check-does-not-follow-the-tddy-web-bundle.md),
[web checks ignore the Rust binaries they exec](../todo/2026-11-07-web-checks-ignore-the-rust-binaries-they-exec.md).

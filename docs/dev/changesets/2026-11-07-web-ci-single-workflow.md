# 2026-11-07 — Web CI is one workflow: build, lint, three test shards (#web-split 6/17, PR #843)

**Type:** CI

Replaces the per-package `web-<package>.yml` callers and the shared `web-package.yml` with
`.github/workflows/web.yml` — checks `Web build`, `Web lint`, `Web tests shard 1/3`…`3/3` — and adds
`.github/workflows/storybook.yml` (`Storybook build`). `scripts/web-ci.ts` discovers the packages from
the root `workspaces` and their own `package.json` (`build:ci`, `typecheck`, `test:ci`,
`cypress:component:ci`, `tddyCi.needs`), plans each shard over the affected packages
(`scripts/web-affected.ts`) and runs it; a later package needs no CI edit.

- `Web lint` type-checks every package except those on `scripts/web-lint-exclusions.json` (tddy-web,
  tddy-rust-typescript-tests, tddy-tauri-web — each with a `docs/dev/todo/` record — and tddy-desktop,
  which has no TypeScript), then runs the web rule tests. `tddy-livekit-rpc-web` and
  `tddy-connectrpc-testkit` gained `typecheck` scripts.
- `scripts/web-package-boundary.ts`, `scripts/web-ci.ts`, `web.yml` and `storybook.yml` are global
  inputs of `web-affected.ts` (closes the #840 boundary-checker TODO); per-package caller workflows
  no longer exist, so they are no longer an input.
- `scripts/web-workflows.test.ts` pins the new shape; `scripts/web-ci.test.ts` the planning and lint rules.
- Required checks become `Web build`, `Web lint`, `Web tests shard 1/3`, `2/3`, `3/3`
  ([ci.md § Web checks](../guides/ci.md#web-checks)); the ruleset edit is an admin step.

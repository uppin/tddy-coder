# 2026-11-07 — `tddy-rpc-playground-web`: the RPC playground screen in its own package (#web-split 9/17, PR #846)

**Type:** Architecture change

Adds `packages/tddy-rpc-playground-web` (workspace entry, `bun.lock`) with `RpcPlaygroundScreen`, the
reflection registry, invocation and protobuf-JSON helpers, their `bun test` suites and two Cypress
specs. `tddy-web` keeps `RpcPlaygroundAppPage` as wiring. The package joins `.github/workflows/web.yml`
by workspace discovery (no caller workflow); `Web lint` excludes it for one error it compiles through
in `tddy-client-web`.
Deviations from the plan: exported names are `buildRegistry`/`findMethod` (not
`buildServiceRegistry`); the screen's URL state moved onto the navigation port so it could leave the
app; the menu-entry spec stayed with the shell.
Package detail: [tddy-rpc-playground-web](../../packages/tddy-rpc-playground-web/docs/changesets/2026-11-07-rpc-playground-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-rpc-playground-package.md).
Product description: [rpc-playground-package.md](../ft/web/rpc-playground-package.md).
Deferred: [lint exclusion](../todo/2026-11-07-rpc-playground-web-does-not-typecheck.md).

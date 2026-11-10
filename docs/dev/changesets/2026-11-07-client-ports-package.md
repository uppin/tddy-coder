# 2026-11-07 — `tddy-client-web`: RPC interfaces and app ports (#web-split 5/16, PR #842)

**Type:** Feature / Refactor

Adds `packages/tddy-client-web` (workspace entry, `bun.lock`, `web-tddy-client-web.yml`): generated RPC
code (moved from `tddy-web/src/gen`, drift gate retargeted), the connection model and registry, and the
clients / navigation / auth ports with the pre-split hook names. `tddy-web` imports generated code and
hooks from it (~480 files, import paths only) and supplies the ports through `rpc/appClientsPorts.tsx`.
Package detail: [packages/tddy-client-web/docs](../../packages/tddy-client-web/docs/changesets/2026-11-07-client-ports-package.md).
`tddy-web` detail: [packages/tddy-web/docs/changesets](../../packages/tddy-web/docs/changesets/2026-11-07-client-ports-package.md).
Product description: [client-ports-package.md](../ft/web/client-ports-package.md).

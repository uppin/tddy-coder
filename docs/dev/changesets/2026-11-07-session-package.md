# 2026-11-07 — `tddy-session-web`: session seams and helpers (#web-split 15/17, PR #851)

**Type:** Architecture change

Adds `packages/tddy-session-web` (workspace entry, `bun.lock`) with the seams session screens use to
reach the app — the navigation port for location, a `SessionShell` slot for the app's shell, and
create-session's passphrase slot — and the session helpers (indicator, notifications, labels,
grouping, sorting, selection, connection status, traffic meter, transport flavour, LiveKit ping).
`tddy-web` fills the slots once in `AppClientsPorts`. The package joins `.github/workflows/web.yml`
by workspace discovery (caller workflow deleted), pre-bundles through `scripts/vite-prebundle.ts`,
and typechecks clean.
Deviations from the plan: the session node was split in four by capability (developer, 2026-11-07)
after its first green found 150 files and ~40 unplanned reaches into `tddy-web`; this node moves no
session component — the runtime (#860), inspector (#861) and drawer (#862) do. `shellQuote`,
`toolShortcuts`, `uploadProgress` and `clipboard` stayed with `tddy-terminal-web`, which owns them.
At wrap: the root `HostStartPassphraseScope` the cascade had restored was removed (one provider,
in `AppClientsPorts`).
Package detail: [tddy-session-web](../../packages/tddy-session-web/docs/changesets/2026-11-07-session-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-session-package.md),
[tddy-client-web](../../packages/tddy-client-web/docs/changesets/2026-11-07-session-package.md).
Product description: [session-package.md](../ft/web/session-package.md).

# 2026-11-07 — `tddy-hosts-web`: hosts, VMs, host stats and the desktop overlays in their own package (#web-split 12/17, PR #848)

**Type:** Architecture change

Adds `packages/tddy-hosts-web` (workspace entry, `bun.lock`) with the Hosts screen and its rows, host
key continuity, the passphrase and desktop sign-in dialogs, host stats (hook, indicators,
`HostStatsFooter`), the screen-sharing, VNC and host-desktop overlays, and the VM screen — 17 Cypress
specs and their suites with it, plus the hosts docs. `tddy-web` keeps `HostsAppPage`, `VmsAppPage`,
`HostStartPassphrasePrompt` and `SessionHostStatsFooter` as wiring; `formatTraffic` moved to
`tddy-ui-web` so both sides share it. The package joins `.github/workflows/web.yml` by workspace
discovery (caller workflow deleted) and pre-bundles through `scripts/vite-prebundle.ts` like every
package; `Web lint` excludes it for the one `tddy-tauri-web` error it compiles through.
Behaviour fix: the desktop sign-in dialog rendered under the "Connecting…" layer (both `z-50`, the
dialog first), so in the app the operator could not reach it; it now renders on top, pinned by
`HostDesktopSignInAcceptance` running with the app stylesheet.
Deviations from the plan: 17 specs moved, not ~30 — `HostConnection*`, `HostDirectoryAcceptance`,
`HostLabelIdentityAcceptance`, `HostSelectionSurvivesReload` (app wiring) and the screen-sharing
input/fullscreen specs (the LiveKit-backed input stream) stay in `tddy-web`; `HostsAppPage` split into
the package's `HostsContent` and the shell; `HostStatsFooter` takes slots instead of session props;
`HostDesktopOverlay` joins the bridge's room through `MediaPort.usePresenterRoom`.
Package detail: [tddy-hosts-web](../../packages/tddy-hosts-web/docs/changesets/2026-11-07-hosts-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-hosts-package.md),
[tddy-ui-web](../../packages/tddy-ui-web/docs/changesets/2026-11-07-hosts-package.md).
Product description: [hosts-package.md](../ft/web/hosts-package.md).

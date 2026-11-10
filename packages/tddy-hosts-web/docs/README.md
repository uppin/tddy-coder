# tddy-hosts-web documentation

Hosts and VMs, and the host-level surfaces other screens show:

- **Hosts screen** — `HostsContent` (the `ListKnownHosts` container), `HostsScreen` and its rows
  (`HostRowTelemetry`, `HostRowTooling` / `HostRowToolingCell`, `HostRowSshAgent`, `HostRowSshConfig`,
  `HostRowRemoteDesktop`, `HostAddKeyAction`), host key continuity (`HostKeyContinuityNotices`,
  `hostKeyPinning`, `hostKeyFingerprint`), the passphrase dialog and the prompt hooks
  (`useHostPrompts`, `useHostPromptAnswer`, `encryptForHost`). See [hosts-screen.md](hosts-screen.md).
- **Host stats** — `useHostStats` over `hostStatsSubscription`, the indicators (`CpuCoresIndicator`,
  `DiskSpaceIndicator`, `MemoryIndicator`, `LoadAverageIndicator`, `hostStatsFormat`) and
  `HostStatsFooter`, which takes `leading` / `trailing` slots — the session traffic strip and upload
  progress are `tddy-web`'s (`SessionHostStatsFooter`). See [host-stats-streaming.md](host-stats-streaming.md).
- **Desktops** — `HostDesktopOverlay` (joins the bridge's room through `MediaPort.usePresenterRoom`,
  asks the sign-in question through `HostDesktopSignInDialog`), `ScreenSharingOverlay` and `VncOverlay`
  (render the port's `RemoteVideo`, forward input through `MediaPort.useScreenSharingInput`;
  `keyboardLock`, `screenSharingStartFailure`).
- **VMs** — `VmsScreen`, `DefineVmPanel`, `DemoVmControls`.

`HostsAppPage`, `VmsAppPage` and `HostStartPassphrasePrompt` stay in `tddy-web` (shell and wiring). The
package imports nothing from `tddy-web` and nothing from LiveKit (`src/packageBoundary.test.ts`).
Diagnostics: `tddyDebug` namespace `tddy:hosts`.

**Layering.** The desktop overlay's connecting layer, the screen-sharing overlay and its dialogs are
all `fixed inset-0 z-50`; at equal `z-index` the later element paints on top, so the sign-in dialog is
rendered after the layer it is asked over. `HostDesktopSignInAcceptance` runs with the app's
stylesheet to pin that.

- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` (host-key pinning and fingerprints, encryption for a host, row formatting, telemetry
routing, stats/prompt subscriptions, keyboard lock, the boundary rule) and `bun run cypress:component`
— 17 component specs from the package's own `cypress.config.ts`, mounted with `cy.mountWithClients`
through `cypress/support/mountWithHosts.tsx` (presence and media doubles). The specs whose point is app
wiring or the LiveKit input stream stay in `tddy-web`. CI: `.github/workflows/web.yml` (`Web lint`
typechecks it; the `Web tests` shards run its suites).

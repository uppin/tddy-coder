# tddy-hosts-web

Hosts and VMs: `HostsScreen` and its rows (telemetry, tooling, SSH agent/config, remote desktop), host
key continuity, passphrase and desktop sign-in dialogs, the host stats footer and indicators, the
screen-sharing and VNC overlays (media through `MediaPort`), `VmsScreen`, `DefineVmPanel`, `DemoVmControls`.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).

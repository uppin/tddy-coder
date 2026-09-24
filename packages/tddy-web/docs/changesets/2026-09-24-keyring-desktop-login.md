# 2026-09-24 — The declared sign-in flow and the device-code screen

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`ClientConfig.authFlow` is an `AuthFlowDeclaration` (`"redirect"`, `"device"`, `"none"` for an absent `auth_flow`, `{ unrecognised }`), read from `/api/config` and `GetClientConfig` alike. `DaemonLoginScreen` (extracted from `index.tsx`) offers only the declared flow, and neither for `none` or an unknown value. `DeviceLoginPanel` and `useAuth`'s `startDeviceLogin` run the device-code flow: code and link, polling no faster than the granted interval, slow-down widening, denial, expiry, a fresh code; a non-positive interval fails the attempt. `checkWholeSession` refuses a session missing its user or either token in both flows and stores nothing. Pinned by `DeviceLoginAcceptance.cy.tsx`, `RedirectLoginAcceptance.cy.tsx` and `clientConfig.test.ts`. Detail: [daemon-sign-in.md](../daemon-sign-in.md).

# 2026-11-07 — Session seams and helpers (#web-split 15/17, PR #851)

New package, first of four session nodes. Adds the `SessionShell` slot and the seam rules
(`src/sessionSeams.test.ts`: location only through the navigation port, the shell only through its
slot, the passphrase prompt only through create-session's slot). Moved from `tddy-web` with their
suites: `lib/{sessionIndicator, sessionParticipantMetadata, sessionsRefreshBridge, sessionReadsLocalPaths}`,
`sessionNotificationRegistry`, `utils/{crossHostSessions, projectHomeGroups, sessionProjectGroups,
sessionDrawerLabel, sessionSort, sessionDisplay, sessionSelection, connectionStatusForSession,
sessionStackGroups, sessionProjectTable}`, `rpc/{trafficMeter, daemonTransportFlavour, livekitPing,
useSessionNotifications}`. `useSessionNotifications` reads the token through the auth port;
`sessionProjectTable` imports `ProjectEntry` from `project_pb`.

At wrap: the package typechecks clean — the builders take message-init overrides and the custom
matcher's `Matchers` declaration matches bun's type parameters (4 errors → 0).

# tddy-session-web documentation

The session package, filled in four `#web-split` nodes: this one (15a, PR #851) carries the seams and
the helpers; the session runtime and terminals (#860), the inspector (#861) and the drawer (#862)
follow.

## Seams

Session screens depend on three things the application owns, and reach each only through a slot the
app fills (pinned by `src/sessionSeams.test.ts`):

| Seam | Slot | Filled by |
|---|---|---|
| the app location and its route params | `tddy-client-web`'s `NavigationPort` (`useAppLocation`) and route builders | `tddy-web`'s hash navigation port |
| the screen shell (header, menu, daemon selector, avatar) | `SessionShellProvider` / `SessionShell` (`src/sessionShell.tsx`) | `tddy-web`'s `AppShell` |
| the start-passphrase prompt | `tddy-session-create-web`'s `StartPassphraseProvider` | `tddy-web`'s `HostStartPassphrasePrompt` |

`tddy-web` fills the shell and passphrase slots once, in `AppClientsPorts` (under `AuthProvider`), so
the app and `tddy-web`'s Cypress `mount` both have them; a missing provider throws naming itself.

## Helpers

| Path | Contents |
|---|---|
| `src/lib/` | `sessionIndicator` (the drawer dot), `sessionNotificationRegistry`, `sessionParticipantMetadata`, `sessionsRefreshBridge`, `sessionReadsLocalPaths` |
| `src/utils/` | `crossHostSessions`, `projectHomeGroups`, `sessionProjectGroups`, `sessionDrawerLabel` (mirrored case for case by `tddy-changeset`'s `session_display_label`), `sessionSort`, `sessionDisplay`, `sessionSelection`, `connectionStatusForSession`, `sessionStackGroups`, `sessionProjectTable` |
| `src/rpc/` | `trafficMeter`, `daemonTransportFlavour`, `livekitPing`, `useSessionNotifications` (token through the auth port) |

Product behaviour: [docs/ft/web/session-drawer.md](../../../docs/ft/web/session-drawer.md).

- **Changeset history**: [changesets/](changesets/)

## Tests

`bun test src` — the helpers' suites, the seam rules and the boundary rule (builders and the
`toContainSessionIdsInOrder` matcher in `test-utils/`). No Cypress specs yet: the session components
and their specs arrive with #860–#862. CI: `.github/workflows/web.yml` (`Web lint` typechecks it; a
`Web tests` shard runs its suites).

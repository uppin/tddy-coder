# tddy-session-create-web documentation

The create-session flow: `CreateSessionPane`, `CreateSessionDialog`, `ProjectHomeSetupPane`, every field
and section, their hooks, the request builder (`buildStartSessionRequest`, `buildProjectHomeStartSpec`),
codebase placement, attachment staging (`attachments/`) and start progress. `tddy-web` mounts it; it
imports nothing from `tddy-web` or `livekit-client` (`src/packageBoundary.test.ts`).

- **Provider field (vendor or provider account for a CLI session)**: [create-session-provider.md](create-session-provider.md)
- **Start progress, cancelling a start, keep-alive events**: [start-progress.md](start-progress.md)
- **Passphrase prompt while a start clones (a slot the host application fills)**: [start-passphrase-prompt.md](start-passphrase-prompt.md)
- **Changeset history**: [changesets/](changesets/)
- **Open code issues**: [code-issues/](code-issues/)

## What it consumes

- `tddy-client-web` — generated clients, `useDaemonClient` / `useHostClient` / `useHostFanOut`, the host
  connection status, `rpcFailureText`, `fileUploadChunks`, `safeTestIdPart`.
- `tddy-ui-web` — primitives and `WorktreeFileTree`.
- From the host application: a `StartPassphraseProvider` above every create-session surface.

## Tests

`bun test src` (helpers, the boundary rule, the passphrase slot) and `bun run cypress:component` — the
component specs mount with `cy.mountWithClients` and per-host in-memory backends (`cypress/support/mounts.tsx`).
CI: `.github/workflows/web.yml` (`Web lint` typechecks it; the `Web tests` shards run its suites).

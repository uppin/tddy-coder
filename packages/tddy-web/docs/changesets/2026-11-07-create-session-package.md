# 2026-11-07 — Create-session comes from `tddy-session-create-web` (#web-split 7/17, PR #844)

- The create-session modules moved to `tddy-session-create-web` (its changeset); the drawer, main pane,
  PR-stack screen, agent picker, roster and terminal tabs import them from the package.
- `HostStartPassphrasePrompt` (the old `StartPassphrasePrompt`, now in `components/hosts/`) fills the
  package's passphrase slot through `HostStartPassphraseScope`, mounted once at the app root
  (`index.tsx`) and in the Cypress `mount`.
- `rpcFailure`, `randomId`, `fileUploadChunks` and `worktreeFilesApi` come from `tddy-client-web`;
  `WorktreeFileTree` from `tddy-ui-web`.
- Specs: `CreateSessionClonePassphrase{Acceptance,Mount}` and `ProjectHomeSetupLinkAcceptance` stay here
  (they pin the Hosts dialog and the drawer); `SessionIndexingIndicatorAcceptance` is the header half of
  the old `SessionStartAndIndexingProgress`. Docs: `create-session-provider.md` and
  `start-passphrase-prompt.md` moved to the package; `session-start-and-indexing-progress.md` split into
  the package's `start-progress.md` and this package's `indexing-indicator.md`.

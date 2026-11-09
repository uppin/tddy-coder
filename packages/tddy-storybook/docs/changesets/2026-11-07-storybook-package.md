# 2026-11-07 — One Storybook for every web package (#web-split 19/20, PR #852)

New package. `.storybook/main.ts` moved from `tddy-web`; `preview.tsx` mounts every story in the port
doubles. The `Button`, `GhosttyTerminal`, `AgentChat` and `AccountsScreen` stories moved here with their
ids unchanged; new stories for `HostsScreen`, `PlannedPrPanel`, `SessionConnectionOverlay`,
`BranchConflictDialog`, `RpcPlaygroundScreen` and `ParticipantList`. `App.stories.tsx` was deleted: it
rendered `tddy-web`'s `App`, which this package may not import. Story placement and story render rules
under `bun test`.

At wrap: `storybook.yml` builds this package (`STORYBOOK_PACKAGE: tddy-storybook`) and its caller
workflow is gone; `main.ts` uses the shared `scripts/vite-prebundle.ts` list; a `typecheck` script
joins it to `Web lint` (clean after typing the render test's stories and the `ParticipantList`
story's capability set).

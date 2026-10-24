# 2026-10-24 — Warp in the Create session pane

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md). See [Create-session Provider field](../create-session-provider.md).

- `SessionType` includes `"warp-cli"`; `SESSION_TYPE_OPTIONS` gains `{ id: "warp-cli", label: "Warp" }`.
- `providerHarnessFor` (`useCreateSessionModel.ts`) is `null` for warp-cli, so no Provider field; no
  model field is mounted; `useCreateSessionSubmit` enables Create with only a project.
- `createSessionRequest.ts`: warp-cli shares the opencode/muse request branch with `model: ""`.

Test: `cypress/component/CreateSessionWarpAcceptance.cy.tsx`.

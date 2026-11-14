# 2026-11-14 — Start an orchestrator from the project entry

**Type:** Feature · `#orchestrators` 8/11 ([#885](https://github.com/uppin/tddy-coder/pull/885))

A "+" on each project header in the session drawer opens the orchestrator composer at
`#/sessions/project/<projectId>/new`: a prompt box centred vertically in the detail pane, with
model, effort, provider account, host and attachments under it. Sending starts an orchestrator
for the project with the text as its initial prompt and selects it; a refusal keeps the text and
shows the daemon's reason. Every listed project gets a group, so the "+" is always reachable.

- `tddy-session-web` — `SessionProjectGroupHeader` (toggle + "+" + actions slot), `withEveryProjectGroup`, drawer and screen routing.
- `tddy-session-create-web` — `OrchestratorComposer`, `useOrchestratorComposerStart`, `buildOrchestratorComposerRequest`, `CreateSessionHostField` extracted from `CreateSessionHostAndProjectFields`.
- `tddy-client-web` — `sessionsProjectNewPath` / `parseSessionsProjectNew`; `tddy-web` re-exports them.
- `tddy-storybook` — `OrchestratorComposer` story.

Backlog cleared: `2026-10-23-session-drawer-grew-past-500-by-project-groups.md` (SessionDrawer.tsx
grew past 500 lines) — the drawer is in `tddy-session-web` at 351 lines and the code issue it
pointed to no longer exists.

Code issues: `CreateSessionPane.tsx` re-measured at 591 lines, unchanged by this PR (not split:
stack-shared with later `#orchestrators` nodes; see
`packages/tddy-session-create-web/docs/code-issues/oversized-file-create-session-pane.md`).

Packages: [tddy-session-web](../../../packages/tddy-session-web/docs/changesets/2026-11-14-orchestrator-composer.md), [tddy-session-create-web](../../../packages/tddy-session-create-web/docs/changesets/2026-11-14-orchestrator-composer.md), [tddy-client-web](../../../packages/tddy-client-web/docs/changesets/2026-11-14-orchestrator-composer.md). Product: [Session drawer § Orchestrator composer](../../ft/web/session-drawer.md#orchestrator-composer).

# 2026-10-24 — main is set up with the New-session form
**Type:** Fix

`CreateSessionPane` gains a main variant (`projectHome`, `createSessionPaneProps.ts`): project and
host fixed, host/project selects, branch, stack parent, attachments and placement hidden, types
disabled with `whyMainCannotRun` (`projectHomeSessionTypes.ts`, shown by `CreateSessionTypeToggle`'s
`disabledReason`), and **Start main session** handing `buildProjectHomeStartSpec`
(`createSessionRequest.ts`) to `onStart`. `ProjectHomeSetupPane` wraps it (replacing warning,
unknown-project and unreachable-host notices); `useEnsureProjectHome` gains `start` and `onNoMain`;
`useProjectHomeStartSubmit` selects and attaches the started session. `ProjectHomeInfo.homeSessionId`
replaces the home-agent label (`projectHomesOnHost`, `projectHomeSlot` by id), the entry is labelled
with main's model and offers **Replace main**; routing adds `/sessions/project-home/<id>/replace`
(`parseSessionsProjectHomeSetup`). The Projects card's `ProjectHomeAgent` picker and
`?project=` focus are removed for `ProjectMainSessionLink`. Generated `project_pb.ts` /
`session_pb.ts` regenerated. See
[session drawer § Pinned main entry](../../../../docs/ft/web/session-drawer.md#pinned-main-entry) and
[projects-screen § Main session link](../projects-screen.md#main-session-link).

Code issues: no record in this package touched, none deleted. `CreateSessionPane.tsx` (498) and
`SessionsDrawerScreen.tsx` (497) stay under the 500-line budget, tracked by
`docs/dev/todo/2026-10-23-session-drawer-grew-past-500-by-project-groups.md` (kept). Deferred:
`docs/dev/todo/2026-10-24-project-card-setup-link-ignores-card-host.md`.

The branch-wide re-measure (`/analyze-code-issues` on tddy-agent-launch, tddy-daemon-rpc and tddy-projects at `4348f160a` against `c1df8e142`, commit `f3f1c93ac`) attributes no finding to this change; `start_session_on_request` in tddy-agent-launch improved from 393 to 384 lines.

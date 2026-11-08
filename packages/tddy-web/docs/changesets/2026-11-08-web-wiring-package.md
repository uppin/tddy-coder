# 2026-11-08 — `tddy-web` is the wiring package (#web-split 20/20, PR #853)

`index.tsx` renders `composeApp(appHandler({ window }))`: one app container (`tddy-react-commons`)
whose handler builds the navigation, LiveKit presence/media/room-RPC and native-file-drop ports, over
the routes now in `src/app/App.tsx`. The port implementations that are still React state moved under
`src/app/rpc/` and `src/app/auth/`; `AuthCallback` and `HostStartPassphrasePrompt` to `src/app/`;
`TasksDrawerScreen` / `StandaloneConnectionScreen` became `TasksAppPage` / `StandaloneConnectionAppPage`.
`AppClientsPorts` has no fallback: outside a composed app it throws naming the missing ports
(`app/portsInScope.ts`). `src/wiringBoundary.test.ts` rejects any feature module under `src/`;
`AppComposition.cy.tsx` routes every screen through the composed app. Leftovers moved to their owners
(`SessionWorkflowStatusCells` → session, `fileUploadChunks.test.ts` → livekit,
`prStackOrchestrators.test.ts` → prstack). At wrap (rebased onto #852) the composed tree no longer
wraps `App` in `HostStartPassphraseScope` — `AppPortSlots` provides the prompt — and the
clients/auth-in-the-handler follow-up is recorded.

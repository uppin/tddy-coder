# 2026-11-07 — Session route builders, auth-flow declaration and `useActionFailure` (#web-split 13/17, PR #849)

Lowered from `tddy-web` so `tddy-workspace-web` can use them without importing the app:
`appRoutes.ts` (`sessionsDrawerPathForSession`, `sessionsProjectHomeSetupPath`, `singleSegmentAfter`,
`decodeSegment`), `authFlow.ts` (`AuthFlow`, `AuthFlowDeclaration`) and `useActionFailure.ts`.

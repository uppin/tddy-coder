# 2026-11-07 — The PR-stack screens move here (#web-split 14/17, PR #850)

New package. `components/sessions/prstack/` and `WorkflowChatScreen` moved from `tddy-web` with their
`bun test` suites, `resolveNodeSession` (+ suite) and the `PrStackScreen` code-issue record; 3 specs
moved and 3 direct-mount specs were added (`PrStackScreenDirectMount`, `PrStackChatDirectMount`,
`PlannedPrListStartSessionDirectMount`). `WorkflowChatPresenterRoomAcceptance` mounts the screen with a
media-port double. `PrStackScreen` no longer mounts `HostStartPassphraseScope`; `SessionMetadata`
became the package's `StackChildMetadata`.

At wrap: the package typechecks clean — `test-utils/builders.ts` takes `MessageInitShape` overrides, so
the suites' plain branch-session literals are valid inits, and the direct-mount specs build their RPC
responses with `create(method.output, …)` (19 errors → 0). `PrStackScreen.tsx` 702 → 701 lines.

# 2026-11-07 — Session runtime and terminals moved to `tddy-session-web` (#web-split 16/17, PR #860)

`SessionMainPane`, the drawer and the inspector import the runtime, registry, terminal tab strip and
attachment hooks from `tddy-session-web`. `SessionMainPane` and its six specs stay (they move with the
inspector, #861); `sessionRuntimeRegistryMetadataMerge.test.ts` stays (it builds a `SessionManager`,
which moves with the drawer). `SessionAgentConversationLayer` stays and imports
`useConversationHostAttachment` from the package. The dropped `cy.wait("@generateToken")` in the moved
`SessionConnectionAcceptance` is covered by `tddy-livekit-web`'s `sessionConnection.test.ts`.

# 2026-11-07 — The session runtime and its terminals move here (#web-split 16/17, PR #860)

From `tddy-web`'s `components/sessions/`: `SessionRuntime`, `SessionChildRuntime`,
`DormantSessionRuntime`, `DormantAgentTerminal`, `sessionRuntimeRegistry`, `GrpcSessionTerminal`,
`useSessionTerminalFeed`, `SessionTerminalTabs`, `useTmuxSessions`, `attachClaim`, and the helpers only
the runtime uses (`sessionRuntimeProps`, `useSessionTerminals`, `useSessionAttachment`,
`useChildSessions`, `useAgentTerminalFocus`, `useSessionPaneFullscreen`, `agentConversationTabs`,
`SessionPaneChrome`, `TerminalControlOverlay`, `SessionConnectionOverlay`, `NewTerminalMenu`,
`tmuxSessionName`, `rpc/connections/{hostServedSession,useConnectionStatus}`, `lib/terminalGridMeasure`);
`useConversationHostAttachment` is new, cut out of `tddy-web`'s `SessionAgentConversationLayer`. 15
specs moved (including `useSessionAttachment.cy.tsx`) and run on the port doubles. New dependencies:
`tddy-terminal-web`, `tddy-session-create-web` (workspace) and `lucide-react`.

At wrap: `bun.lock` regenerated against the registry — identical to the green's hand edit. Typecheck
4 → 1 (`src/vite-env.d.ts` for the ghostty wasm import, a widened `wake` in `TerminalInputAckAcceptance`,
and `tddy-terminal-web`'s driver stubs `confirm` with `.as` before `.returns`); the one left is
`tddy-tauri-web`'s, so `Web lint` excludes the package on that record.

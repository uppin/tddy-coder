# 2026-11-07 — Agent chat moved to `tddy-chat-web` (#web-split 10/17, PR #847)

`components/chat/` keeps only `AgentChat.stories.tsx` (Storybook is this package's), importing from
`tddy-chat-web`. Session, PR-stack and models components (`SessionActivitySidebar`,
`SessionAgentRosterPane`, `SessionAgentTree`, `SessionInspectorActivityTab`,
`SessionWithActivitySidebar`, `WorkflowChatScreen`, `PrStackChat`, `ModelChatDialog`) import chat,
the activity registry and `rosterHalfOf` from the package. Chat-named specs that mount session
components stay here for `feature/web-split/session`, with copies of `agentChatPage`,
`agentActivityPage` and the ACP RPC support.

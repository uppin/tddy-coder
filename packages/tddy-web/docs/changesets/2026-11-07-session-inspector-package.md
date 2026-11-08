# 2026-11-07 — Inspector, agents, code pane and SessionMainPane moved to `tddy-session-web` (#web-split 17/17, PR #861)

The drawer (`SessionsDrawerScreen` and its hooks) imports `SessionMainPane`, the inspector and the agent
layers from the package. `inspectorState`, `jailedCodebaseHalf`, `useMeterSnapshot` and
`SessionDetailPane` stay — the drawer hooks, status bar and traffic strip read them.
`PrStackPresenterRoomAcceptance` stays here (it pins the app transport's presenter-room token mint).
`react-resizable-panels` and `react-syntax-highlighter` left this manifest with their only users. The
code-navigation, indexing-indicator, agent-conversation and agent-tree docs moved to the package.

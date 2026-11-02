# 2026-11-02 — The "+" menu's tmux section

**Type:** Feature

`useTmuxSessions` lists the host's tmux sessions each time the "+" menu opens (waiting for the
terminal client, dropping overtaken answers); `NewTerminalMenu` renders them as a group of their own
with loading, empty and error rows; `SessionTerminalTabs` labels a tmux tab `tmux: <name>`;
`useSessionTerminals.open(kind, tmuxSession?)` and `SessionTerminal.tmuxSession`. Cypress:
`SessionTerminalTabsTmuxAcceptance.cy.tsx`. Part of [#806](https://github.com/uppin/tddy-coder/pull/806) —
see [Session terminal tabs](../../../../docs/ft/web/session-terminal-tabs.md).

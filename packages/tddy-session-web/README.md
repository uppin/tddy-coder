# tddy-session-web

Sessions: the sessions drawer (groups, project home entries), the session runtime and runtime
registry, the main pane and terminal tabs (`GrpcSessionTerminal`, dormant agent/session tabs, tmux),
the inspector and its tabs, the agent roster/tree/conversation layers, notifications, traffic strip,
status bar, and the code pane (`code/`). It composes the terminal, chat, hosts, workspace, PR-stack and
create-session packages.

It imports nothing from `tddy-web` and nothing from LiveKit; it reaches the outside world through
`tddy-client-web` (generated clients and ports), and renders with `tddy-ui-web`.

Tests: `bun test src` and `bun run cypress:component` (specs mount with `cy.mountWithClients`).

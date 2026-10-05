# 2026-10-05 — `#carve`: the agent topic runs over `AgentRoster` and `AgentHostCallbacks`, in place

**Type:** Architecture

`tddy-session-lifecycle`'s agent-clone and roster topic is converted from `impl DaemonSessionHost` methods
to `impl AgentRoster`, an owned handle over twelve host fields, with the host capabilities that are not
fields behind a five-method `AgentHostCallbacks` trait implemented once on the host. `tddy-session-agents`
gains the two `AgentRosterState` fields the topic reads (`session_admissions`, `model_registry`). No
behaviour change, no crate move, no consumer edited, no crate edge added.

Package entries, with the layout, decisions, acceptance results, measurements and engine gaps:

- [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/docs/changesets/2026-10-05-carve-agent-roster-handle.md)
- [`tddy-session-agents`](../../../packages/tddy-session-agents/docs/changesets/2026-10-05-carve-agent-roster-handle.md)

Numbers: the lifecycle suite is 575 passed, 22 failed (the known macOS-only sandbox and LiveKit suites),
1 ignored, before and after, with an identical failing set; `tddy-session-agents` is 75 passed (no test added); CI at the
final head is green at 8,473 Rust tests, 380 e2e and 2,800 web tests.

The node's engine run (five module moves and re-parents, four renames) found eight gaps, filed as the
`2026-10-05-restructure-*` backlog entries named in the lifecycle entry. The next conversion nodes (split
sessions, stack spawns and jail launches, session start and resume) extend the same recipe; the move of the
agent topic into `tddy-session-agents` follows them.

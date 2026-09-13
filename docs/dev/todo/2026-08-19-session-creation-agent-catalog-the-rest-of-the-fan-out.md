# 2026-08-19 — Session-creation agent catalog — the rest of the fan-out

**Category:** Future enhancement
**Source:** session-agent-host-fan-out changeset, 2026-08-19

The Agent `<select>` now reads `ListAgents` from every common-room host and labels each option with
the host that offers it. These gaps were scoped out of that changeset deliberately.

- **An assistant's assigned tools are not shown, and an assistant is not distinguishable from a
  config agent.** `AgentInfo` is `{ id, label }` (`connection.proto:270`), so the dropdown cannot say
  that `reviewer` is a registry assistant carrying `Read`/`Grep`/`Edit` while `claude` is a config
  allowlist entry. The daemon already has both — `agent_allowlist_rows(&config, &assistants)`
  (`packages/tddy-daemon/src/agent_list_mapping.rs:21`) chains them and throws the distinction away.
  Needs an `AgentInfo.kind` plus `AgentInfo.tools`, or the web keeps guessing.

- **`ListAgentModels` still probes the app-level selected daemon.** `daemon_instance_id` on the
  request participates only in the cache key (`connection_service.rs:8436`), so selecting a peer
  host's agent lists the *selected* host's models for it. Invisible where both hosts run the same
  backends, wrong where they do not. The fix is to route the probe to the agent's host, the way the
  fan-out routes the catalog read.

- **`ListTools` still reads the app-level selected daemon.** A tool session needs `agent`, `toolPath`
  and `model` together, so a peer host's agent can still be unsubmittable for want of a tool path
  that host has.

- **No `timeoutMs` on any fan-out read.** A daemon whose LiveKit *RPC* participant (`daemon-{id}`) is
  absent while its *discovery* participant is present — observed for ~69 minutes — never answers and
  never rejects. `useHostFanOut` (`src/rpc/useHostFanOut.ts`) then leaves that host as neither a row
  nor an error row (`if (!answer) continue`), and the four `useModelRegistryFanOut` reads pin
  themselves at "loading" forever. A caller-supplied `timeoutMs` is the only thing that settles a
  LiveKit unary call the peer never answers; an absent destination identity does not reject the
  publish. Now one fix for both agent fan-outs — `useHostFanOut` is where the read and the
  pending/unreachable row belong — plus a second in `useModelRegistryFanOut`, which does not share
  that hook.

- **`CreateSessionPane.tsx` is 1351 lines.** Pre-existing (1279 before this feature, and the feature
  put its fan-out, its option algebra and its Agent select in files of their own rather than in it),
  but well past the 500-line guideline. It holds nine more controls, the branch loader, the attachment
  uploader, the peer-mode freezing and the submit path. The Agent select shows the shape a split takes
  — a presentational control with explicit props, the state staying in the pane — and 29 specs in
  `CreateSessionPane.cy.tsx` alone make it a change to do deliberately, in its own changeset, not as a
  rider on a feature.

- **`ListAgents` advertises registry assistants that `ListAgentModels` refuses to enumerate.** The
  probe shells out to `tddy-tools list-models`, whose `build_backend`
  (`packages/tddy-tools/src/list_models.rs:89`) matches a fixed six-backend set and otherwise bails
  with `unknown agent "<name>"`. So an assistant is offered as an agent and then has no model list,
  which leaves Create disabled with a probe error. The assistant row already holds its `model_id`, so
  `list_agent_models` can resolve a registry assistant before falling through to the probe.

- **`resolve_specialized_agent_defs` accepts an assistant name in `specialized_agents` that the
  subagent picker never lists.** It resolves through `resolvable_agent_defs()` (YAML + registry),
  while `list_subagents` reads YAML only — so the roster picker and the start path disagree about
  what an assistant is. Assistants carry `replaces: Vec::new()` by design
  (`model_registry/assistant_def.rs`), so listing them in the subagent picker is the wrong fix; the
  question is whether the start path should refuse them there instead. A design call, not a one-liner.

## Re-read at `#unbundle` node 7's wrap (2026-09-12)

Every bullet stands, and the one thing that could have moved did not: `ListAgents`,
`ListAgentModels`, `ListSubagents` and `ListSessions` are all still on
`connection.ConnectionService` (`connection.proto:34, 39, 42, 43`), and `AgentInfo` is still declared
there (`:151`). `#unbundle` node 7 took family B — the *roster*, which is what an attached agent is —
and left the *catalog*, which is what an agent may be attached from.

Two references drifted:

- `resolve_specialized_agent_defs`'s disagreement with `list_subagents` is now a disagreement across
  a crate boundary: the roster side resolves in `packages/tddy-session-agents/src/service.rs`
  through the `AgentCatalog` port, while `list_subagents` stays in `tddy-daemon`. The design call the
  bullet describes is unchanged; the two implementations are now further apart, which makes the
  "quietly disagree" failure mode likelier rather than less.
- `CreateSessionPane.tsx`'s line count is untouched by this node. Its sibling
  `SessionMainPane.tsx` picked up a related problem — a 42-prop interface — recorded separately in
  [2026-09-12-the-acp-replay-framing-is-written-twice.md](./2026-09-12-the-acp-replay-framing-is-written-twice.md).

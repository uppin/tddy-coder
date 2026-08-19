# PRD: Session-creation agent catalog — fleet fan-out with host labels

**Date:** 2026-08-19
**Product area:** Web
**Type:** Feature (multi-host correctness)
**Status:** 🚧 In Progress

## Summary

The "New session" form's **Agent** dropdown lists only the agents offered by the **one** daemon the
app is currently pointed at. Every other host in the common room is invisible, so an agent that
exists on `mac` cannot be selected from a browser pointed at `udoo` — it is not merely hard to find,
it is absent, with nothing on screen saying so.

This changes the Agent dropdown to a **fleet-wide fan-out**: `ConnectionService.ListAgents` is read
from **every daemon in the common room**, each row is **labelled with the host that offers it**, and
**picking an agent sets the session's Host to that agent's host** — so the composed
`StartSession` is valid by construction.

The pattern is not new. `useAvailableAgents` already fans `ListSubagents` out across the fleet for
the *specialized-agent* multi-select in the same form, labels each row with its host, and renders one
error row per host that could not answer. This PRD applies that same shape to the *primary* agent
select, which was left single-host.

## Background

Three RPCs feed the create-session form, and they do not agree on what "host" means:

| RPC | Answers for | Request carries a host? |
|-----|-------------|--------------------------|
| `ListAgents` | the answering daemon's config allowlist + its registry assistants | no field at all |
| `ListSubagents` | the answering daemon's `<tddyhome>/agents/*.yaml` | no field; each answer is **stamped** with the answering daemon's instance id |
| `ListAgentModels` | a probe (`tddy-tools list-models`) run on the answering daemon | `daemon_instance_id`, used **only as a cache key** |

Meanwhile the form holds two unrelated notions of host:

- the **app-level selection** (`selectedInstanceId`, persisted in the `?host=` URL param) — the
  daemon whose `client` every catalog read goes to;
- the in-pane **Host** `<select>` (`daemonInstanceId`) — a *field on the outgoing request*, which
  does not redirect any catalog read.

So an operator who creates an assistant on `mac`, then changes the in-pane Host to `mac`, still sees
`udoo`'s agents: the Host select never moved the read. This was observed directly — `udoo`'s log
answered `list_agents RPC: returning 4 agent(s)` (`claude`, `claude-acp`, `cursor`, `codex`) with the
`mac` assistant nowhere in it, because the read never left `udoo`.

## Proposed changes

### What's changing

1. **New hook `useSelectableAgents(homeClient, homeInstanceId)`** (`src/components/sessions/`) —
   reads `ListAgents` from the home daemon and from every other common-room daemon over its
   `daemon-{instanceId}` RPC identity, each host read **independently**. Returns
   `{ agents, failures }`.

   Because `ListAgents` carries no host field and `AgentInfo` has no host field, the host is
   attributed **client-side**, from the identity the read was addressed to. This is exact — the
   caller chose the destination — and needs no proto or daemon change.

2. **Qualified option values.** Each option's value is `"{id}@{daemonInstanceId}"` when the common
   room advertises daemons, and the bare `id` when it does not (no common room means one host and
   nothing to disambiguate — the same `daemons.length > 0` gate the Host select already uses). Two
   hosts offering `claude` therefore yield two distinct, separately selectable options.

3. **Host labels.** Each option's text is `"{label} · {daemonInstanceId}"`, and the same host is
   exposed for assertion on a per-option testid.

4. **Selecting an agent sets the Host.** `onChange` resolves the picked row and sets both `agent`
   (bare id) and `daemonInstanceId` (the agent's host). The wire format is unchanged:
   `StartSessionRequest.agent` keeps carrying a bare id and `.daemon_instance_id` the host.

5. **Changing the Host re-points the agent.** If the newly chosen host offers an agent of the
   currently selected name, that host's agent stays selected; otherwise the first agent that host
   offers is selected. The pair on screen is therefore never contradictory.

6. **One error row per unlistable host.** A host that refuses or cannot be reached renders one row;
   every other host's agents stay selectable. This is the existing specialized-agent-picker rule,
   which exists precisely because a silent omission is indistinguishable from "that host has no
   agents".

7. **An explicit empty state.** When no host offered an agent and none failed, the select renders a
   single disabled `No agents available` placeholder rather than an empty control — matching the
   Project select, which already does this.

8. **`ListAgents` leaves the mount-time `Promise.all`.** It currently shares one `Promise.all` with
   `ListProjects` and `ListTools`, so any one failure discards all three. Moving the agent read into
   its own hook decouples them as a side effect.

### Assistants are the point of the list

`ListAgents` answers with the responding daemon's **config allowlist plus its registry assistants**
(`agent_allowlist_rows(&config, &assistants)`), and an **assistant already carries its assigned
tools** — it is a provider, a model, a system prompt and a tool set, projected onto a
`SpecializedAgentDef` when the session starts. So the fan-out surfaces every host's assistants
without any daemon change, and that is the case that motivated it: an assistant created on `mac` is
exactly what a browser pointed at `udoo` could not see.

What the fan-out **cannot** show is the assistant's tool set, or that a row is an assistant at all:
`AgentInfo` is `{ id, label }`. Displaying either would need a new `AgentInfo` field and a daemon
change, so it is a follow-up rather than part of this changeset.

### What's staying the same

- **`ListTools`** stays a single-host read against the app-level selected daemon.
- **`ListAgentModels`** stays a single-host read against the app-level selected daemon. Models are
  per-agent, so fanning them out is meaningless; the correct change is *routing* the probe to the
  selected agent's host, which is deliberately out of scope here (see below).
- The **specialized-agent** multi-select and `useAvailableAgents` are untouched.
- The proto (`connection.proto`), the daemon, and every Rust package are untouched. This is a
  **web-only** change.
- The in-pane Host select stays visible and editable; it gains a coupling, not a removal.

## Out of scope

- **Routing `ListAgentModels` to the selected agent's host.** Consequence, stated plainly: selecting
  a peer host's agent lists the *app-level* host's models for it. Where both hosts run the same
  backends this is invisible; where they do not, the model list is wrong.
- **Routing `ListTools` to the selected agent's host.** A tool session needs `agent`, `toolPath` and
  `model` together, so a peer-host agent may still be unsubmittable for want of a tool path that
  host has.
- **`timeoutMs` on the fan-out reads.** A daemon whose LiveKit *RPC* participant is absent while its
  *discovery* participant is present (observed for ~69 minutes) never answers and never rejects, so
  that host renders as neither an agent nor an error. `useAvailableAgents` and
  `useModelRegistryFanOut` have the same gap today; fixing the class of it was considered and
  deferred.
- **`ListAgents` advertising assistants `ListAgentModels` cannot enumerate.** A registry assistant is
  offered as an agent, but the model probe rejects it (`unknown agent "<name>"`) because
  `list_agent_models` shells out to a fixed six-backend set without consulting the registry. The
  assistant row already carries its `model_id`, so the fix is a daemon-side resolve.

All of these are recorded in `docs/dev/TODO.md` § Future Enhancements.

## UX

```
Host:  [ mac ▾ ]                         ← follows the agent
Agent: [ My Assistant · mac         ▾ ]
         Claude · udoo
         Cursor · udoo
         Codex · udoo
         My Assistant · mac              ← picking this sets Host = mac
       server-3: no connection to daemon server-3
```

## Acceptance criteria

- [x] **AC1** — The Agent select lists the agents of **every** common-room daemon, not only the
      app-level selected one.
- [x] **AC2** — Each option names the **host** that offers it.
- [x] **AC3** — Two hosts offering an agent of the **same id** yield two distinct, separately
      selectable options.
- [x] **AC4** — Selecting a peer host's agent **sets the session's Host** to that agent's host.
- [x] **AC5** — The started session carries the **bare** agent id and the **agent's host**
      (`StartSessionRequest.agent`, `.daemon_instance_id`).
- [x] **AC6** — A host that cannot be listed costs **one error row**; every other host's agents stay
      selectable.
- [x] **AC7** — When no host offered an agent and none failed, the select shows a disabled
      `No agents available` placeholder.
- [x] **AC8** — On open, the selection is the **home** daemon's first agent, so merely opening the
      form does not move the session's host.
- [x] **AC9** — Changing the Host to a host that offers the selected agent's name keeps that agent
      selected (now that host's); changing to a host that does not offer it selects that host's first
      agent.
- [x] **AC10** — With no common room (no advertised daemons), option values stay **bare ids** and no
      host label is rendered.
- [x] **AC11** — A **registry assistant** offered by a peer host appears in the list and is
      selectable — the case the change exists for.
- [x] **AC12** — In the **peer-agent spawn** flow the list is scoped to the host the peer will run
      on: only that host's agents are offered, and only its failure is reported. The peer joins an
      orchestrator's worktree, so its host is settled before the form opens. An empty
      `daemon_instance_id` on the orchestrator means "the connected daemon" and resolves to the
      fan-out's home host.

## References

- [Session agent roster](../../daemon/session-agent-roster.md) § Web UI — the fan-out precedent
      (`useAvailableAgents`, AC48/AC49) this mirrors.
- [Tool-session model selection](../tool-session-model-selection.md) — `ListAgentModels`, whose
      single-host behaviour this PRD deliberately leaves alone.
- [Models & agents](../models-and-agents.md) — where registry assistants come from.
- [Daemon selector over LiveKit RPC](../daemon-selector-livekit-rpc.md) — the dual-identity rule
      (`daemon-{instanceId}` for RPC) every peer read depends on.

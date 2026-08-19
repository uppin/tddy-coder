# Session-creation agent catalog — fleet fan-out with host labels

**Product area:** Web
**Status:** Implemented

## Purpose

The "New session" form's **Agent** dropdown lists the agents offered by **every daemon in the common
room**, each option labelled with the host that offers it, and **picking an agent sets the session's
Host** to that agent's host.

Before this, the dropdown listed only the agents of the one daemon the app was pointed at. An
assistant created on `mac` could not be selected from a browser pointed at `udoo` — not merely hard
to find, absent, with nothing on screen saying so. Changing the in-pane **Host** select did not help:
that field is part of the outgoing request and never redirected the catalog read, so `udoo`'s agents
stayed on screen while the session was aimed at `mac`.

## How the fleet is read

`ConnectionService.ListAgents` carries **no routing field**. A daemon answers with its own config
allowlist plus its own registry assistants, and never forwards the request to a peer — so the only
way to see the fleet is to ask every host. The web does exactly that: one client per advertised
daemon, each peer addressed at its `daemon-{instanceId}` RPC identity over the shared common-room
connection (see [Daemon selector over LiveKit RPC](daemon-selector-livekit-rpc.md) for why the RPC
identity and the discovery identity differ).

`AgentInfo` is `{ id, label }` — nothing on the wire says which host answered — so each row is
attributed to the identity its read was **addressed to**. That is exact, because the caller chose the
destination, and it needs no proto or daemon change. It would stop being exact only if a daemon ever
forwarded `ListAgents` to its peers.

Every host is read **independently**: a host that refuses or cannot be reached costs **one error row**
above the select, naming the host and repeating the daemon's own words, while every other host's
agents stay selectable. A silent omission is indistinguishable from "that host has no agents", which
is the whole bug this feature exists to fix.

## What the operator sees

```
Host:  [ mac ▾ ]                         ← follows the agent
Agent: [ My Assistant · mac         ▾ ]
         Claude · udoo
         Cursor · udoo
         Codex · udoo
         My Assistant · mac              ← picking this sets Host = mac
       server-3: no connection to daemon server-3
```

- **Host labels.** An option reads `{label} · {host}` while daemons are advertised, and just `{label}`
  when none are — no common room means one host and nothing to disambiguate.
- **Same name on two hosts.** Two hosts offering `claude` yield two distinct, separately selectable
  options; the option's value is qualified as `{id}@{host}` so a pick is unambiguous.
- **Picking an agent sets the Host.** The wire format is unchanged: `StartSessionRequest.agent` still
  carries the **bare** id and `.daemon_instance_id` the host.
- **Changing the Host re-points the agent** to that host's agent of the same name, else that host's
  first agent, else none. The agent and host on screen are therefore never contradictory — a session
  cannot be composed naming a host that does not offer the agent.
- **On open**, the selection is the connected daemon's first agent, so merely opening the form never
  moves the session's host.
- **Nothing offered and nothing failed** shows a disabled `No agents available` placeholder, matching
  the Project select.

## Assistants are the point of the list

`ListAgents` answers with the responding daemon's config allowlist **plus its registry assistants**,
and an assistant already carries its provider, model, system prompt and tool set (see
[Models & agents](models-and-agents.md)). So the fan-out surfaces every host's assistants with no
daemon change — the case that motivated it.

What it cannot show is an assistant's tools, or that a row is an assistant rather than a config
allowlist entry: `AgentInfo` has no field for either. That needs a proto and daemon change and is
tracked in `docs/dev/TODO.md`.

## The peer-agent spawn flow

When a session is added **to** an existing session, the new session reuses the orchestrator's
worktree, so its project and host are settled before the form opens and the Host select is hidden. The
catalog is therefore **scoped to the host the peer will run on**: only that host's agents are offered,
and only that host's failure is reported — another host's outage is not this session's problem, and a
foreign option cannot be picked.

An orchestrator started without an explicit host carries `daemon_instance_id: ""`. That is not the
absence of a host: a daemon serves an empty id on whichever host the request arrives at, which is the
daemon the browser is connected to. The scoping names that host; the request keeps sending the id it
was given.

## Deliberately unchanged

- **`ListAgentModels`** stays a single-host read against the app-level selected daemon. Models are
  per-agent, so fanning them out is meaningless; the correct change is *routing* the probe to the
  selected agent's host. Consequence today: where two hosts run different backends, a peer host's
  agent lists the wrong host's models. See
  [Tool-session model selection](tool-session-model-selection.md).
- **`ListTools`** stays a single-host read, so a peer host's agent can still be unsubmittable for want
  of a tool path that host has.
- **The specialized-agent multi-select** is unchanged. It fanned `ListSubagents` out already, and both
  fan-outs now run on one shared implementation.
- **The proto, the daemon and every Rust package** are untouched. This is a web-only change.

Each gap above is recorded in `docs/dev/TODO.md` § Future Enhancements.

## References

- [Session agent roster](../daemon/session-agent-roster.md) § Web UI — the fan-out precedent.
- [Models & agents](models-and-agents.md) — where registry assistants come from.
- [Daemon selector over LiveKit RPC](daemon-selector-livekit-rpc.md) — the dual-identity rule.
- Module docs: [host fan-out](../../../packages/tddy-web/docs/host-fan-out.md).

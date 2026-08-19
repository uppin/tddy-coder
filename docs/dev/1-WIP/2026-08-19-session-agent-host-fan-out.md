# Changeset: Session-creation agent catalog — fleet fan-out with host labels

**Date**: 2026-08-19
**Status**: 🚧 In Progress
**Type**: Feature (multi-host correctness)

## Affected Packages

- **tddy-web**: [README.md](../../packages/tddy-web/README.md)
  - `src/components/sessions/useSelectableAgents.ts` (new) — the `ListAgents` fleet fan-out
  - `src/components/sessions/selectableAgentOptions.ts` (new) — the pure option/selection algebra
  - `src/components/sessions/CreateSessionPane.tsx` — the Agent `<select>`, its host coupling, and
    the removal of `listAgents` from the mount-time `Promise.all`
  - `cypress/support/testIds.ts` — per-option and per-host-error testids for the Agent select
  - `cypress/support/pages/createSessionPage.ts` — agent-select page-object methods
  - [changesets.md](../../packages/tddy-web/docs/changesets.md) — changeset index entry

No Rust package, no proto and no daemon change: the host an agent came from is attributable
client-side, from the LiveKit identity the read was addressed to.

## Related Feature Documentation

- [PRD: Session-creation agent catalog — fleet fan-out with host labels](../ft/web/1-WIP/PRD-2026-08-19-session-agent-host-fan-out.md)
- [Session agent roster](../ft/daemon/session-agent-roster.md) § Web UI — the fan-out precedent
- [Models & agents](../ft/web/models-and-agents.md) — where registry assistants come from
- [Tool-session model selection](../ft/web/tool-session-model-selection.md) — `ListAgentModels`,
  left single-host on purpose
- [Daemon selector over LiveKit RPC](../ft/web/daemon-selector-livekit-rpc.md) — the
  `daemon-{instanceId}` RPC-identity rule every peer read depends on

## Summary

Make the "New session" form's **Agent** dropdown speak for the whole fleet instead of for one
daemon: read `ConnectionService.ListAgents` from every common-room host, label each option with the
host that offers it, and set the session's **Host** from the agent that was picked. A **registry
assistant** created on `mac` — a provider, a model, a system prompt and an already-assigned tool set —
becomes selectable from a browser pointed at `udoo`, and the resulting `StartSession` names a host
that can actually resolve it.

## Background

`ListAgents` has no routing field, so a daemon answers for itself only — its config allowlist plus
its own registry assistants. The form reads it once, from the app-level selected daemon's client, and
the in-pane **Host** `<select>` does not redirect that read: it is a field on the outgoing request.
An operator who creates an assistant on `mac` and then sets Host to `mac` still sees `udoo`'s four
agents, with nothing on screen indicating that the catalog and the host disagree. Confirmed from
`udoo`'s log — `list_agents RPC: returning 4 agent(s)`, none of them the `mac` assistant.

The same form already solves this for *specialized* agents: `useAvailableAgents` fans `ListSubagents`
out across the fleet, labels each row with its host, and renders one error row per host that could
not answer. This changeset applies that shape to the primary agent select.

## Technical Changes

### State A — current implementation

`CreateSessionPane.tsx:276-325` — one mount effect, one `Promise.all`:

```ts
Promise.all([
  client.listProjects({ sessionToken }),
  client.listAgents({}),
  client.listTools({}),
]).then(([projectsResp, agentsResp, toolsResp]) => {
  …
  setAgents(agentsResp.agents as AgentInfo[]);
  if (loadedAgents.length > 0) setAgent(loadedAgents[0]!.id);
  …
});
```

`CreateSessionPane.tsx:770-788` — the select, rendered only for `sessionType === "tool"`:

```tsx
<select data-testid="create-session-agent-select" value={agent}
        onChange={(e) => setAgent(e.target.value)}>
  {agents.map((a) => <option key={a.id} value={a.id}>{a.label || a.id}</option>)}
</select>
```

Consequences:

- **One host.** `client` addresses the app-level selected daemon; no other host is read, so no other
  host's assistants exist as far as the form is concerned.
- **Coupled failures.** A failing `ListProjects` or `ListTools` discards the agent list too, and
  vice versa — `Promise.all` rejects as a unit and the `.catch` only `console.debug`s.
- **No host in the option.** `AgentInfo` is `{ id, label }`; nothing on screen says which host.
- **Bare values.** Two hosts offering `claude` could not be told apart by the `<select>`'s value.

### State B — target implementation

**`selectableAgentOptions.ts` (new, pure)** — no React, unit-testable:

```ts
export interface SelectableAgent {
  readonly id: string;            // the bare id the daemon knows it by, sent as `agent`
  readonly label: string;
  readonly daemonInstanceId: string;
}

/**
 * The `<option>` value. Qualified while hosts are advertised, because two hosts routinely offer an
 * agent called `claude` and a bare value cannot say which was picked; bare when no host is
 * advertised, where there is one host and nothing to disambiguate.
 */
export function selectableAgentValue(agent: SelectableAgent, hostsAdvertised: boolean): string;

/** The option's text: the label, and the offering host when there is more than one host to name. */
export function selectableAgentText(agent: SelectableAgent, hostsAdvertised: boolean): string;

/**
 * The agent to select once `host` runs the session: the one of the same name that host offers, else
 * that host's first, else none — so the pair on screen is never contradictory.
 */
export function agentForHost(
  agents: readonly SelectableAgent[],
  host: string,
  currentAgentId: string,
): SelectableAgent | null;
```

**`useSelectableAgents.ts` (new)** — `useSelectableAgents(homeClient, homeInstanceId)`, modelled on
`useAvailableAgents`: one read per common-room daemon, peers addressed at
`daemonRpcIdentity(instanceId)` over the shared common-room transport, each answer held per host so
one unreachable host is one error row. Returns
`{ agents: SelectableAgent[], failures: AgentHostFailure[] }`, agents in host order (home first),
de-duplicated by `id@daemonInstanceId`.

**`CreateSessionPane.tsx`** —

- `listAgents` leaves the `Promise.all`; `agents`/`setAgents` state is replaced by the hook.
- The select's `value` is derived from the `(agent, daemonInstanceId)` pair rather than stored, so
  the control cannot display a host the request will not carry.
- `onChange` resolves the picked row from the fan-out and sets **both** `agent` (bare id) and
  `daemonInstanceId` (the agent's host). No string parsing: the row is looked up, not decoded.
- A Host change re-points the agent through `agentForHost`.
- Empty fan-out and no failures → one disabled `No agents available` option.
- Per-host failures render above the select, one row each.

### Delta

| Area | Change |
|------|--------|
| `selectableAgentOptions.ts` | new pure module: option value, option text, host re-point |
| `useSelectableAgents.ts` | new hook: `ListAgents` fan-out, per-host answers, per-host failures |
| `CreateSessionPane.tsx` | Agent select rebuilt on the hook; host coupling both ways; empty state; `listAgents` out of the `Promise.all` |
| `testIds.ts` | `createSessionAgentOption(value)`, `createSessionAgentSelectHostError(host)`, `TEST_IDS.createSessionAgentEmptyOption` |
| `createSessionPage.ts` | `agentOption`, `agentSelectHostError`, `agentEmptyOption`, `selectAgent` |
| existing specs | `CreateSessionAcceptance.cy.tsx` and `CreateSessionAutoClosesDrawer.cy.tsx` provide daemons and `.select("claude")`; their values become qualified. The three specs that mount without a `SelectedDaemonProvider` keep bare values by AC10 and are untouched. |

## Implementation Milestones

- [x] `selectableAgentOptions.ts` — pure module, unit-tested
- [x] `useSelectableAgents.ts` — fan-out hook
- [x] Agent select rebuilt on the hook, with host labels and the empty state
- [x] Selecting an agent sets the Host; changing the Host re-points the agent
- [x] `listAgents` removed from the mount-time `Promise.all`
- [x] testids + page-object methods
- [x] Fixture specs updated to qualified values — **seven**, not the two predicted (`CreateSessionAcceptance`, `AutoClosesDrawer`, `BranchConflictAcceptance`, `AttachmentProgress`, `HostDocumentPicker`, `AttachmentsAcceptance`, `HostSelectionAcceptance`); the last two also needed a LiveKit-capable mount so their advertised peer host can answer `ListAgents`

## Testing Plan

### Test level

**Cypress component tests** are the right level for every acceptance criterion here. The behaviour
under test is a *composition*: what the form renders after N per-host RPCs land, and which host the
resulting `StartSession` names. That composition lives in React state and in the LiveKit
transport-factory seam, so nothing below the component can observe it, and nothing above it (e2e)
can drive two hosts with one unreachable.

`mountWithPerDaemonLiveKitRpc` is the seam that makes it possible: it maps a `daemon-{id}` identity
to its own in-memory backend, so host A and host B answer differently — which plain `mountWithRpc`
cannot do (its factory ignores the identity and returns one transport).

**bun:test unit tests** cover `selectableAgentOptions.ts`, where the algebra is pure: value
qualification, option text, and the host re-point rule. These are cheap and exhaustive, so the
component specs are left to assert behaviour rather than enumerate cases.

### Options considered

| Option | Trade-off |
|--------|-----------|
| Cypress component + pure-module unit tests (**chosen**) | Drives real per-host RPC answers through the real component; the branchy selection rule is pinned exhaustively in fast unit tests. |
| Component tests only | Every re-point case becomes a full mount — slow, and the rule is pinned indirectly. |
| e2e against two live daemons | Cannot script an unreachable host, needs two real daemons, and answers no question the component seam does not. |
| Daemon-side Rust tests | Wrong package: no daemon behaviour changes. |

### Coverage requirements

Every AC1–AC11 has at least one named acceptance test. AC9 has two (name offered / name not
offered), because the two halves of the rule fail independently.

## Acceptance Tests

### tddy-web — Cypress component (`packages/tddy-web/cypress/component/CreateSessionAgentHostFanOut.cy.tsx`)

- [x] **Acceptance**: `lists the agents of every connected host, labelled by the host that offers them` (AC1, AC2)
- [x] **Acceptance**: `offers an agent of the same id on two hosts as two separate choices` (AC3)
- [x] **Acceptance**: `offers a peer host's assistant as an agent a session can start as` (AC11)
- [x] **Acceptance**: `sets the session host to the host of the agent that was picked` (AC4)
- [x] **Acceptance**: `starts the session with the bare agent id and the agent's host` (AC5)
- [x] **Acceptance**: `costs one row when a host cannot be listed rather than the whole select` (AC6)
- [x] **Acceptance**: `offers a disabled placeholder when no host has an agent` (AC7)
- [x] **Acceptance**: `opens on the home host's first agent so opening the form does not move the host` (AC8)
- [x] **Acceptance**: `keeps the selected agent when the new host offers one of the same name` (AC9)
- [x] **Acceptance**: `selects the new host's first agent when it does not offer the selected one` (AC9)
- [x] **Acceptance**: `leaves option values bare when no daemons are advertised` (AC10)
- [x] **Acceptance**: `offers only the agents of the host the peer will run on` (AC12)
- [x] **Acceptance**: `starts a peer as an agent the host it runs on offers` (AC12)
- [x] **Acceptance**: `stays silent about a host the peer will not run on failing to answer` (AC12)

### tddy-web — unit (`packages/tddy-web/src/components/sessions/selectableAgentOptions.test.ts`)

- [x] **Unit**: `qualifies an option value with the host that offers the agent`
- [x] **Unit**: `leaves an option value bare when no host is advertised`
- [x] **Unit**: `names the offering host in the option text`
- [x] **Unit**: `leaves the option text unqualified when no host is advertised`
- [x] **Unit**: `falls back to the id when an agent carries no label`
- [x] **Unit**: `keeps the agent of the same name when the new host offers one`
- [x] **Unit**: `falls to the new host's first agent when it does not offer the selected name`
- [x] **Unit**: `selects no agent when the new host offers none`
- [x] **Unit**: `ignores an agent of the same name on a host that is not the new one`

## Technical Debt & Production Readiness

Recorded in [docs/dev/TODO.md](../TODO.md) § Future Enhancements, under this changeset's name:

- **An assistant's assigned tools are not shown, and an assistant is not distinguishable from a
  config agent.** `AgentInfo` is `{ id, label }`. Surfacing either needs a new `AgentInfo` field and
  a daemon change.
- `ListAgentModels` still probes the app-level selected daemon, so a peer host's agent lists the
  wrong host's models where the two hosts run different backends.
- `ListTools` still reads the app-level selected daemon, so a peer host's agent may be unsubmittable
  for want of a tool path that host has.
- No `timeoutMs` on any of the fan-out reads. A daemon whose LiveKit *RPC* participant is missing
  while its *discovery* participant is present never answers and never rejects, so that host renders
  as neither an agent nor an error row. Shared with `useAvailableAgents` and
  `useModelRegistryFanOut`.
- `ListAgents` advertises registry assistants that `ListAgentModels` refuses to enumerate
  (`unknown agent "<name>"`), because `list_agent_models` shells out to a fixed backend set without
  consulting the registry. The assistant row already holds its `model_id`, so the resolve is
  daemon-side.

## Validation Results

`/pr-wrap` steps 1–5, 2026-08-19.

### Test evidence (re-run after every refactor)

| Suite | Result |
|---|---|
| `selectableAgentOptions.test.ts` (`bun test`) | **9 pass / 0 fail** |
| `CreateSessionAgentHostFanOut.cy.tsx` | **14 pass / 0 fail** |
| `CreateSession*` + `*Peer*` (23 specs) | **175 pass / 0 fail** |
| `PrStack*` | **190 pass / 0 fail** |
| `bun test` package-wide (105 files) | **900 pass / 0 fail** |
| `bun run build` | ✓ 23.12s |
| `tsc --noEmit`, production files | 0 errors |

### Fixed during wrap

- **Test quality (step 2)**: two raw `byTestId(TEST_IDS.createSessionSubmitBtn)` calls in the new
  spec replaced with the page object's `submitButton()`, the now-unused import dropped, and a
  redundant `selectedAgentValue` assertion removed from the peer-options test.
- **Clean code (step 4)**: the `<option>` `key` in `CreateSessionPane` now reads
  `selectableAgentValue(a, true)` instead of re-spelling `` `${a.id}@${a.daemonInstanceId}` ``, so the
  key format cannot drift from the value format.

### Open, needs the developer's call

- ⚠️ **`agentHostInstanceId`'s `||` chain is fallback-shaped** —
  `effectiveDaemonInstanceId || selectedInstanceId || ""`. It resolves the documented empty-id
  sentinel ("whichever daemon the browser is connected to") for scoping only, and the wire still
  carries the empty id; it masks no error. But CLAUDE.md forbids fallbacks *without explicit
  developer consent*, so it is flagged rather than assumed. `SessionsDrawerPeerSpawn.cy.tsx` (whose
  orchestrator carries `daemonInstanceId: ""`) is what caught the need for it.
- ⚠️ **~70% structural duplication with `useAvailableAgents.ts`** — 103 differing lines of ~340
  normalized, and `useModelRegistryFanOut` is a third instance of the same shape. Extracting a
  generic host-fan-out hook would rewrite two working callers, so it needs consent before it
  happens; recorded as a candidate, not done.
- ⚠️ **`CreateSessionPane.tsx` is 1389 lines** (>500 guideline). Pre-existing; this change grew it
  by ~110. A split is its own changeset.

## Decisions & Trade-offs

- **Host attributed client-side, not stamped by the daemon.** `ListSubagents` stamps
  `daemon_instance_id` on its answers, and mirroring that would mean a proto field, a daemon change
  and a version skew where old daemons answer without a host. The caller already knows the identity
  it addressed, so attribution is exact without any of that. Reconsider only if a daemon ever
  forwards `ListAgents` to its peers, at which point an answer stops speaking for the responder.

- **Qualified `<option>` values, and the wire format left alone.** The select must distinguish
  `claude` on two hosts, so its value is qualified. `StartSessionRequest` already carries the host
  separately, so `agent` keeps its bare id — a qualified id on the wire would be a daemon change for
  no gain, and the daemon refuses non-local agent ids at start today.

- **Bare values with no common room.** Gated on `daemons.length > 0`, the same condition that already
  hides the Host select. One host means nothing to disambiguate, and it keeps the three specs that
  mount without a provider honest about single-host behaviour rather than papering over it.

- **Selecting an agent moves the Host, rather than filtering the list to the Host.** Filtering makes
  a peer host's agent undiscoverable until the operator guesses which host to try, which is the bug
  being fixed. Disabling off-host options was also considered: it shows what exists but still makes
  reaching it a two-step guess.

- **Changing the Host re-points the agent instead of clearing it.** A cleared agent disables Create
  with no explanation for an operator who only changed hosts. The re-point is the generalisation of
  the auto-select the form already does on load, not a fallback that hides a failure.

- **The list stays config agents *and* assistants, not assistants alone.** `claude` / `cursor` /
  `codex` are how an ordinary tool session is started; dropping them to leave only assistants would
  remove the form's existing capability. Assistants are what the fan-out makes newly reachable, not
  what the list narrows to.

- **No `timeoutMs`, deliberately and against advice.** The class of bug is real and observed. Fixing
  it means touching two hooks outside this changeset, and it was scoped out; it is recorded above.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 11/11 red, each on its own missing selector
- [x] USER REVIEW — acceptance tests (approved 2026-08-19)
- [x] TDD Red — write failing unit/integration tests — 9/9 red on missing implementation
- [x] TDD Green — implement with quality code — 11/11 acceptance + 9/9 unit green
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete — one extra cycle for AC12 (peer flow)
- [x] Run all tests — verify 100% pass — `CreateSession*.cy.tsx` 163/163, `bun test` package-wide 900/900, `bun run build` clean
- [x] Validate changes — `/pr-wrap` steps 1–5, see **Validation Results**
- [ ] USER REVIEW — development complete
- [x] Linting and type checking — `bun run build` ✓ 23.12s; `tsc --noEmit` clean on all three production files; no Rust touched, so `cargo fmt`/`clippy`/`test` have nothing to act on
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps

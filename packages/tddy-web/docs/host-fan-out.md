# Cross-daemon fan-out (`src/rpc/useHostFanOut.ts`)

A daemon answers a list RPC for **itself**. The requests carry no routing field and a daemon never
forwards one to a peer, so any surface that has to show the whole fleet asks every daemon itself.

`useHostFanOut` is that read, once.

Feature docs: [Session-creation agent catalog](../../../docs/ft/web/session-agent-catalog-fan-out.md),
[Session agent roster](../../../docs/ft/daemon/session-agent-roster.md) § Web UI.

## What the hook owns

| Property | How |
|---|---|
| One client per host | The home client is passed in; each peer gets `liveKitFactory(room, daemonRpcIdentity(id))` — the identity a daemon actually serves RPC on, not its discovery identity |
| Isolated reads | One answer held per host, so an unreachable host is one `HostReadFailure` row and never an empty list. A failed read and an empty answer are separate values — only the second may render as an absence |
| One row per identity | Rows are de-duplicated by the reader's key, so a host reached twice (its own client and its common-room identity) does not double its rows |
| No stray reads | One `AbortController` per host list; unmounting or a changed host list aborts the reads in flight. The peer set is depended on as a newline-joined key, because `daemons` is rebuilt on every participant event while its contents rarely change |

## What the caller owns

A module-scope `HostReader<C, T>`: `clientFor` (which service), `read` (the RPC, plus the host id for
rows whose wire format does not carry the answering host), and `keyOf` (what makes two rows the same
row across hosts).

The reader is held in a ref, so a caller re-describing the same service cannot restart the reads —
an inline reader would otherwise turn every render into a fresh round of RPCs. A reader swapped for a
genuinely different one takes effect on the next read, which is why it belongs at module scope.

## Readers

| Hook | RPC | Row key | Host attribution |
|---|---|---|---|
| `sessions/useSelectableAgents.ts` | `ListAgents` | `id@daemonInstanceId` | **client-side** — `AgentInfo` has no host field, so each row is stamped with the identity the read was addressed to |
| `sessions/useAvailableAgents.ts` | `ListSubagents` | the daemon-minted `agent_id` | **on the wire** — the serving daemon stamps `daemon_instance_id` |

`components/models/useModelRegistryFanOut.ts` fans out the same way but is **not** a reader: it reads
several RPCs per host into one composite snapshot, merges by provider identity rather than by row key,
and issues writes back to the owning host. It shares the shape, not the contract. Both share
`noConnectionTo`, so an unreachable host reads the same everywhere.

## Known gap

No `timeoutMs` on any read. A daemon whose LiveKit *RPC* participant is absent while its *discovery*
participant is present never answers and never rejects — an absent destination identity does not
reject the publish — so that host renders as neither a row nor an error row. Tracked in
`docs/dev/TODO.md`.

# Host stats streaming (`src/rpc/useHostStats.ts`, `src/rpc/hostStatsSubscription.ts`)

`ConnectionService.StreamHostStats` is a server-stream carrying a host's per-core CPU and the
free/total capacity of its project directory. The daemon owns the cadence: one snapshot on subscribe,
then CPU every 5 s and disk every 60 s, each event carrying both.

Two surfaces consume it — the Host Stats Footer, which follows the daemon selector, and the Hosts
screen, which shows a reading per row. One hook serves both.

## `useHostStats(hostId?)` — three call shapes

| Call | Subscribes to |
|---|---|
| `useHostStats()` | the selected daemon, via `useDaemonClient` |
| `useHostStats(id)` | that host, via `useHostClient(ConnectionService, id)` |
| `useHostStats(null)` | nothing |

`null` and `undefined` are **not** interchangeable, and the distinction is load-bearing. `undefined`
means "follow the selector"; `null` means "this caller gets no feed". Collapsing them
(`hostId ?? undefined`) would report the selected daemon's CPU under an unreachable host's name.

The readings reset whenever the resolved client changes. Without that, a host that went away and came
back would re-show its pre-outage figures as a live reading until the new feed's first frame — and
indefinitely if it never reported.

## `subscribeHostStats` — why the loop is not a `for await`

The loop lives in its own function, and it iterates the stream **manually**. Both facts follow from
one constraint: a subscription has to be closable.

`@connectrpc/connect` hands a generated client's server-stream back as an iterable whose iterator has
`next` and nothing else — its `handleStreamResponse` builds it that way deliberately, commented
"Create a new iterable to omit throw/return". So `iterator.return()` does not exist in production and
cannot end a call. Neither can a `for await` loop's `break`: parked awaiting a frame that may never
arrive, it never reaches one, so a host that subscribes and then stays silent is never let go of.

Cancellation therefore runs through an `AbortSignal`, as it does in `useTaskListStream`,
`useHostFanOut`, `useLiveKitRooms` and `useTerminalControl`. `subscribeHostStats` takes
`open(signal)`, the caller threads the signal into the RPC call, and `unsubscribe()` aborts it.
Aborting rejects the parked `next()`, so the loop unwinds and releases. Both transports honour it:
the envelope transport ends the call and drops its per-request registration on abort.

Two details that are easy to lose in a refactor:

- The unsubscribed flag is re-checked **after** the await, so a frame already in flight when the
  caller let go is dropped rather than delivered to a component that stopped listening.
- Only `open()` and `next()` sit inside `catch`. The caller's own handler does not, so a bug there
  surfaces instead of being mistaken for a dropped feed. A teardown's AbortError is not reported.

Tests live in `src/rpc/hostStatsSubscription.test.ts`. One fake is deliberately shaped like the real
client — `next` only, no `return` — because a fake offering `return()` lets a subscription that never
cancels look correct.

## `telemetryFeedFor` — which host a row reads

`src/components/hosts/hostTelemetryState.ts` answers, for one Hosts row, which host its cell should
read: the row's own `instanceId` when it is online **and** routable, otherwise `null`.

It is a pure function, and separate from the component, because it is the one property a component
test cannot observe. Every host in `mountWithRpc` shares one in-memory transport and
`StreamHostStatsRequest` names no host, so a cell reading the selected daemon for every row renders
identical DOM and opens an identical number of streams.

"Routable" is host-directory membership (`useDaemons`), not a non-null client. A registered common
room answers `connectHost` for **any** host id — roster membership is the directory's business, not a
provider's — so a non-null client is no evidence a host is reachable. `useHostFanOut` reads its peers
by the same rule.

## `HostRowTelemetry` — four states, none of them a number

| Row state | Cell |
|---|---|
| offline | an offline marker; nothing subscribed |
| online, nothing routes to it | an unavailable marker |
| online, subscribed, no frame yet | a pending marker |
| online, subscribed, reading in hand | CPU bars and free disk |

The last two are decided **per metric**: a reading carrying disk but no CPU leaves the CPU slot
pending rather than drawing `CpuCoresIndicator` with an empty array, which is indistinguishable from
every core at 0 %. Offline and unavailable are separately addressable so neither can be confused with
the em dash `DiskSpaceIndicator` renders for a null reading.

The cell reuses `CpuCoresIndicator` and `DiskSpaceIndicator` unchanged, and carries the row's
per-core percentages as `data-core-{n}` attributes — the indicator's own bar ids are per-core and
would collide across rows.

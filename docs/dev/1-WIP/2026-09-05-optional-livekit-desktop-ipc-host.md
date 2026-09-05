# Changeset: optional-livekit-desktop-ipc-host

**Stack:** `optional-livekit` — node 7 of 7 (parents: `capability-gating`, `multi-connection-ipc`;
PR base `feature/optional-livekit/capability-gating`, with `multi-connection-ipc` merged in through
the local integration ref `stack-int/desktop-ipc-host`)
PR: [#443](https://github.com/uppin/tddy-coder/pull/443)
PRD: [`2026-09-05-optional-livekit-desktop-ipc-host-prd.md`](2026-09-05-optional-livekit-desktop-ipc-host-prd.md)
Discovery: [`2026-09-05-optional-livekit-desktop-ipc-host-initial-discovery.md`](2026-09-05-optional-livekit-desktop-ipc-host-initial-discovery.md)

## State A

Everything is in place; nothing is wired.

- Nodes 1–4: `ConnectionProvider` registry, `HostConnection`/`SessionConnection` with capabilities,
  a source-merged host directory (`room` gone from the context), and media/presence surfaces gated
  on capability.
- Node 6: `WebviewRpcHost` holds a map of connections keyed by client epoch; `ConnectionTarget` is
  `Daemon | Session { session_id }`; `RosterResolver` maps a target to a roster;
  `tddy_rpc_disconnect` releases one connection; `tddy-tauri-web` exposes `openConnection(target)`
  and `close()`; `daemonTransport.ts` keeps a per-target bridge registry.
- The desktop build registers **no** provider and **no** directory source, so its own host is still
  only reachable the way `tddy-web` knows: over a LiveKit common room it does not join by default.
- `DaemonConfigService.GetClientConfig` already carries `livekitUrl`, `commonRoom` and
  `daemonInstanceId`, and `clientConfig.ts` already reads it over IPC in the desktop flavour.

## State B

- The desktop build registers an `IpcConnectionProvider` and a `LocalHostDirectorySource`.
  `tddy-web` imports neither; they arrive through node 1's registry and node 2's source list, so the
  browser bundle is unchanged and no `isDesktop` branch exists anywhere.
- The local host is reached over the `Daemon`-targeted IPC connection, advertising `{"rpc"}`.
- `openSession` on the local host opens a `Session { session_id }` IPC connection — separate and
  concurrent, released on `close()`. Several attached sessions hold several connections; detaching
  one leaves the others.
- No room, participant, token or LiveKit identity exists on the IPC path.
- With no LiveKit configuration: no room, no token, exactly one host, media and presence surfaces
  absent rather than broken.
- With LiveKit configured: local host over IPC **and** common-room peers over LiveKit, both usable
  without a reload. A LiveKit failure degrades the peers only.
- In a browser, the desktop machine's host is still reached over LiveKit — nothing changes there.

## Responsibility

- `IpcConnectionProvider` — host and session connections over node 6's addressed IPC, with
  `{"rpc"}` capabilities and correct `close()` lifecycle.
- `LocalHostDirectorySource` — the local host descriptor, from `daemonInstanceId`.
- The desktop build's registration point (its entry module), and provider precedence for the host it
  claims.
- Making the LiveKit directory source and connection provider genuinely inert when `livekitUrl` /
  `commonRoom` are empty, in the desktop app as in the browser.
- Cypress component coverage for both desktop configurations, and the `tddy-desktop` e2e run.

## Boundaries

- Does **not** define, rename or widen anything in `src/rpc/connections/*` or
  `src/rpc/hostDirectory/*` — nodes 1–3 own those interfaces. This PR implements against them.
- Does **not** change capability gating decisions or add a gated surface — node 4's.
- Does **not** change `WebviewRpcHost`, `ConnectionTarget`, `RosterResolver`, the IPC commands or
  `tddy-tauri-web`'s bridge API — node 6 owns all of it. If a node-6 signature is wrong here, that is
  a plan change to raise, not to patch from this branch.
- Does **not** merge the terminal components — node 5's, and its sibling. A terminal over IPC works
  through whichever terminal path is current when this lands.
- Does **not** carry media over IPC, and does **not** open a second LiveKit connection for the
  desktop's own host. `{"rpc"}` only; the surfaces are absent, and that is the deliberate answer.
- Does **not** change any proto, the daemon's LiveKit behaviour, or the common-room advertisement.
- Adds **no npm dependency and no Rust dependency**.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `capability-gating` (#440) | and through it nodes 1–3: `ConnectionProvider` + registry, `HostConnection`/`SessionConnection` with `capabilities`, `openSession` + `SessionAttachmentHint`, the source-merged directory with `useHostPresence`, and `useHasCapability` gating every media/presence surface | registers a provider and a source into those registries; relies on gating to make `{"rpc"}`-only render correctly with no further work | add to `ConnectionCapability`; change `openSession`, the hint, routing or the client cache; change `useHasCapability`; re-gate a surface |
| `multi-connection-ipc` (#442) | `ConnectionTarget` (`Daemon` \| `Session{session_id}`), `RosterResolver`, the connection map keyed by client epoch, `tddy_rpc_disconnect`, `openConnection(target)` + `close()`, the per-target bridge registry in `daemonTransport.ts` | the provider opens one `Daemon` connection and one `Session` connection per attached session, and closes each on detach | change the target enum, the resolver, the IPC commands, the bridge API, or `createDefaultDaemonTransport`'s signature |

**Sequencing:** this node needs **both** parents' contract commits before its tests compile. Node 6
is a root and pushes independently of the web chain — its push is what unblocks half of this node.
This PR is only offered for merge once both parents have merged, at which point its base collapses to
`master`.

## Draft PR contract

Lands first:

1. `IpcConnectionProvider` and `LocalHostDirectorySource` signatures, and the desktop registration
   point.
2. Failing Cypress acceptance tests, driven through an in-memory IPC bridge double:
   - the local host appears with `{"rpc"}` and is selectable with no LiveKit configuration;
   - `openSession` opens a `Session`-targeted connection, and `close()` releases it;
   - several attached sessions hold several concurrent connections; detaching one leaves the others;
   - no `Room` is constructed and no token minted anywhere on the IPC path;
   - with LiveKit configured, the directory shows local **and** peers, and each is reached over its
     own wire;
   - a LiveKit source failure leaves the local host fully functional.
3. Failing test: in the browser flavour, no IPC provider is registered and the desktop machine's host
   is still reached over LiveKit.

Implementation lands in the same PR under `/green`. **Not a merge candidate on the contract alone.**

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Create failing acceptance tests — `cypress/component/DesktopIpcHostAcceptance.cy.tsx`
- [x] Run acceptance tests (verify they fail) — 7/8 failing; the 8th is a green regression guard, below
- [x] USER REVIEW — acceptance tests — waived 2026-09-05 (run wave 2 straight through)
- [x] TDD Red — write failing unit/integration tests — `src/rpc/connections/localHost.test.ts`
- [ ] Implement production code making tests pass (`/green`)
- [ ] `/validate-changes`
- [ ] `/pr-wrap`

### Baseline and red status at the contract commit

`bun run --filter tddy-web test:unit` — 971 pass, 17 fail before this node's tests; all 17 are
inherited red from #437 (6), #439 (5) and #440 (6). This node adds 7 more.

| Suite | Result |
|---|---|
| `src/rpc/connections/localHost.test.ts` | **7 tests, 7 failing** |
| `cypress/component/DesktopIpcHostAcceptance.cy.tsx` | 8 tests, **7 failing**, 1 green |

The failures are on this node's own `TODO(desktop-ipc-host)` bodies —
`createIpcConnectionProvider`, `liveKitIsConfigured`.

**The one green test is deliberate and is recorded rather than hidden.** *"a browser … reaches the
desktop machine's host over LiveKit"* exercises no code of this node's: it drives only the LiveKit
stand-in. It is a regression guard for the requirement that the browser path is untouched — the
strongest form of which is structural, since `tddy-web` never imports `localHost.ts` and a browser
bundle therefore cannot contain the IPC provider even by accident. A later change that registered the
IPC provider everywhere would fail here.

**Two scoping decisions forced by the diamond**, both recorded so `/green` does not rediscover them:

1. **`createLocalHostDirectorySource` is not in this contract.** It returns node 2's
   `HostDirectorySource`, which is not on this branch's PR head — node 2 reaches this worktree only
   through `stack-int/desktop-ipc-host`. Importing it would leave this PR unable to compile on its
   own head. Its intended behaviour is written down in `localHost.ts` beside where it will go.
2. **The acceptance spec resolves hosts through a local `resolveThrough` helper, not through
   `ConnectionProviderRegistry`.** The registry is node 1's and still unimplemented; provider
   precedence is precisely what these specs assert, so it is spelled out here rather than borrowed
   from an unimplemented dependency. Driving through the registry would have made every failure read
   as node 1's.

### Commands

```bash
./dev bun run --filter tddy-web test:unit
./dev bun run --filter tddy-web cypress:component
cargo test -p tddy-tauri-rpc                       # unchanged here; regression guard
./desktop-dev                                      # manual, BOTH configurations
./dev bun run --filter tddy-desktop e2e            # signIn.e2e.ts — NOT in the CI gate
```

Both desktop configurations — LiveKit configured and not — must be run and reported. `tddy-desktop`
and Cypress e2e are outside the four required checks, so a green PR proves neither. Installs via
`bun run local-registry-install`; the public npm registry is unavailable.

## Successor PRs

None — top of the stack.

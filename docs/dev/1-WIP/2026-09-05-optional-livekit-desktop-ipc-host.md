# Changeset: optional-livekit-desktop-ipc-host

**Stack:** `optional-livekit` — node 7 of 7 (parents: `capability-gating`, `multi-connection-ipc`;
PR base `feature/optional-livekit/multi-connection-ipc`). The diamond this node was planned as has
since been **linearized**: `multi-connection-ipc` sits above `terminal-convergence` above
`capability-gating`, so both parents are now plain ancestors and the local `stack-int/desktop-ipc-host`
integration ref is no longer needed. Live PR state is authoritative; the earlier header described the
plan, not the branch.
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

- The desktop build registers an `IpcConnectionProvider` and a `LocalHostDirectorySource`; a browser
  registers neither. They arrive through node 1's registry and node 2's source list, so nothing in
  `tddy-web`'s screens names a wire, and there is no *second* notion of "is this the desktop" — the
  one `daemonTransportFlavour` already answers is reused. *(Corrected during `/green`: this first read
  "`tddy-web` imports neither … no `isDesktop` branch exists anywhere", which assumed a desktop-only
  entry module that does not exist. See "Item 4" below.)*
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
- `LocalHostDirectorySource` — the local host descriptor, from `daemonInstanceId`. **Corrected at
  `/green`:** it is merged *after* the LiveKit source, not ahead of it — see "Directory ordering"
  below. Connection precedence is unchanged and still puts IPC first; the two are separate registries.
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
- [x] Implement production code making tests pass (`/green`) — all five items delivered and green
- [x] `/validate-changes` — found one CRITICAL production bug; fixed and pinned, see below
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
stand-in. It is a regression guard for the requirement that the browser path is untouched. *(Corrected
during `/green`: this said the guarantee's strongest form was structural, "since `tddy-web` never
imports `localHost.ts`". It does import it, and always would have — one bundle. The guarantee is
behavioural, and this guard plus "registers nothing at all for a page a browser loaded" are what
enforce it.)* A later change that registered the IPC provider everywhere would fail here.

**Two scoping decisions forced by the diamond**, both recorded so `/green` does not rediscover them:

1. **`createLocalHostDirectorySource` is not in this contract.** It returns node 2's
   `HostDirectorySource`, which is not on this branch's PR head — node 2 reaches this worktree only
   through `stack-int/desktop-ipc-host`. Importing it would leave this PR unable to compile on its
   own head. Its intended behaviour is written down in `localHost.ts` beside where it will go.
   **RESOLVED at `/green`:** the linearization put node 2 in this branch's ancestry, so
   `HostDirectorySource` is on the PR head and the function is implemented here with its own tests.
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

## Green status — 2026-09-05

Delivered and verified on this branch, rebased onto `multi-connection-ipc`:

| Responsibility item | State |
|---|---|
| 1. `IpcConnectionProvider` — host + session connections, `{"rpc"}`, `close()` lifecycle | **done** |
| 2. `LocalHostDirectorySource` from `daemonInstanceId` | **done** (deferral resolved by the linearization) |
| 3. `liveKitIsConfigured` | **done** |
| 4. The desktop build's registration point and provider precedence | **done** — `localHostRegistrationFor` + `LocalHostConnections`, gated on the transport flavour |
| 5. LiveKit source/provider inert when unconfigured | **already true at HEAD**; verified, no change needed |

Evidence:

| Suite | Result |
|---|---|
| `src/rpc/connections/localHost.test.ts` | **14 pass, 0 fail** (the 7 contract tests unchanged + 7 added) |
| `bun run --filter tddy-web test:unit` | **1091 pass, 0 fail** |
| `cypress/component/DesktopIpcHostAcceptance.cy.tsx` | **8 pass, 0 fail** |
| `cargo test -p tddy-tauri-rpc` | ok — node 6 untouched |

The 17 inherited failures recorded at the contract commit (#437, #439, #440) are **all gone**: those
parents' green phases have landed, and this branch now carries them.

Item 5 needed no code. With `livekitUrl` / `commonRoom` empty, `useCommonRoom.ts:41` short-circuits
before `generateToken` and before `roomFactory()`, so no `Room` is constructed and no token minted;
`liveKitSource` reports `idle`, and `directoryStatusOf` never derives `error` from an `idle` source.
Parent nodes' own tests already pin it. `liveKitIsConfigured` is this node's named statement of the
rule; it is deliberately **not** wired into `useCommonRoom`, because that would make `tddy-web` import
`localHost.ts` and destroy the structural guarantee the browser regression guard exists to protect.

### Item 4 — resolved by reusing the existing transport-flavour check

The plan's premise did not hold. `packages/tddy-desktop/src-tauri/tauri.conf.json` sets
`frontendDist: "../../tddy-web/dist"`, so the Tauri shell loads **the same bundle the daemon serves
to browsers**: one build, one entry (`packages/tddy-web/src/index.tsx`). There is no desktop-only
entry module, and there never was — so `## State B`'s "no `isDesktop` branch exists anywhere" was
describing a build layout this repository does not have.

**Decision (operator, 2026-09-05): reuse the runtime host check that already exists.**
`localHostRegistrationFor(win, daemonInstanceId)` delegates to `daemonTransportFlavour(win)` and
returns `null` for a browser page. That is not a new branch: `daemonTransportFlavour.ts` already
answers exactly this question, once, and already uses the answer to decide how the page reaches its
own daemon. Asking a second, differently-worded question would have invented a way for the two
answers to disagree. The rejected alternative was a second bundle with its own Vite entry, which
would have matched the plan's literal wording at the cost of `tauri.conf.json`, `./install` and
`./publish.sh` changes — a build-architecture change outside this node's scope.

**The browser guarantee is therefore behavioural, not structural.** `index.tsx` does import
`localHost.ts`, and this module always was in the browser's bundle; what keeps a browser off the IPC
path is that nothing is ever registered there. The doc comments in `localHost.ts` and above the
acceptance spec's browser guard were rewritten to say so — the previous wording claimed a structural
guarantee the code does not provide.

Wiring: `LocalHostConnections` (`src/rpc/connections/localHostRegistration.tsx`) follows
`LiveKitConnections` exactly — `useConnectionProviders()`, provider in a `useRef`, `register()` called
unconditionally every render. It builds `transportFor` from `useTrafficMeterRegistry()` and
`useAuthTokenGate()`, which is the registration-site responsibility that made `transportFor`
defaultless in the first place. It wraps `SelectedDaemonProvider`, so `ipc` registers ahead of
`livekit`.

### Directory ordering — a corrected plan assumption

`## Responsibility` said the local source should be preferred **over** a common-room advertisement of
the same machine. It is merged **after** the LiveKit source instead
(`[liveKitSource, ...hostSources, servingSource]`), and the reason is recorded at the ordering site.

The plan assumed the local source would be the *richer* account of the machine. It is not: it is
built from `GetClientConfig`, which carries `daemonInstanceId` and **no** `repos_base_path` or
`max_attachment_bytes` (`clientConfig.ts`), and teaching it to is a proto change this node may not
make. Ahead of the room it would shadow the advertisement with a strictly poorer copy and cost the
local host its attachment cap — so `useSessionAttachments.ts:243` would compute no cap and line 249
would silently stop refusing over-cap files. That is the identical regression `selectedDaemon.tsx`'s
pre-existing comment already guards against for the serving source, so this ordering follows an
established rule in that file rather than a new one.

Nothing is lost. Connection precedence is a **separate** registry and still puts IPC first, so the
local host is still reached in-process; only which descriptor names it changes, and only when a
common room also advertises the same machine. With no LiveKit configured the LiveKit source
contributes nothing, so the local source names the host — which is the case this whole stack exists
for. Pinned by *"does not shadow a common room's richer account of the same machine"*. Move it to the
front the day the daemon advertises those fields to its own page.

### Known limitations, recorded rather than hidden

- **`LocalHostConnections` sits inside the `isAuthenticated` branch**, so the IPC provider registers
  after sign-in, in the same place the LiveKit provider does. `LocalHostRegistration`'s doc says the
  IPC path has no sign-in gate and gives "a usable host from its first paint": that is true of the
  registration *data*, but `App` gates the whole daemon-mode UI on auth today. Lifting that gate is
  not this node's change.
- **`liveKitIsConfigured` has no production caller.** Item 5 is already enforced by `useCommonRoom`'s
  own guard, which is a parent node's file. The function is this node's named statement of the rule;
  it is documented as such rather than given a redundant call site or wired in by reaching into a
  parent's surface.

## Validation Results — /validate-changes, 2026-09-05

### Stack gate

| Check | Result |
|---|---|
| Stack branch | Yes — planned, base `feature/optional-livekit/multi-connection-ipc` |
| `/pr-stack-rebase` | ✅ Rebased (base had moved twice: `0024a826`, then node 6's doc wrap) |
| Leak check (`origin/<base>..HEAD` is this PR only) | ✅ Clean — 4 commits, 9 files |
| Parent-owned files intact | ✅ No deletions anywhere in the diff |
| `## Dependencies` not implemented here | ✅ No parent-owned file in the diff; `tddy-tauri-web` / `tddy-tauri-rpc` untouched |
| `## Boundaries` respected | ✅ Nothing in `src/rpc/connections/*` or `src/rpc/hostDirectory/*` beyond this node's own files |
| No dependent's behaviour | ✅ Top node; no dependents |

`selectedDaemon.tsx` **is** modified, and it is node 2's file. That is inside the boundary — `## Boundaries`
names only `src/rpc/connections/*` and `src/rpc/hostDirectory/*`, and `## State B` requires the source to
arrive through node 2's source list. It is additive (one optional prop, one merge-array line). Recorded
because #438 is open and this is live conflict surface.

### CRITICAL — found and fixed: the local host could not issue a single RPC

`RpcTransportProvider` builds the daemon transport eagerly, and on the desktop that opens the
`DAEMON_TARGET` bridge and **connects it** (`transport.ts:104`). `IpcChannel` then called
`openConnection(DAEMON_TARGET)` — which memoises, returning that same bridge — and built a *second*
transport over it, calling `connect()` again under the same epoch. `multi_host.rs:113` answers
`EpochInUse`, so every call through the local `HostConnection` failed. Worse, `transport.ts:239` assigns
`registration` unconditionally, so the shared daemon bridge's registration became the rejected promise and
its `close()` stopped calling `tddy_rpc_disconnect` — disarming release for the page's own connection.

**Cause: node 6's `0024a826` landed on this branch mid-implementation.** Before it each transport minted
its own epoch, so a second transport over one bridge worked; making a bridge own one epoch turned a latent
design flaw into a hard failure. `WebviewIpcBridge.connect`'s doc now states the invariant outright.

**Fix:** `LocalHostWiring.hostTransport` — the host connection *uses* the page's existing daemon transport
(`useHttpTransport()`) rather than building one, and no longer references `DAEMON_TARGET` at all.
`transportFor` is now for **session** bridges only, which `openConnection(sessionTarget(id))` genuinely
mints fresh. No node-6 file was touched.

**Pinned:** `"opens no connection of its own to reach the daemon"`. Verified by mutation — reintroducing
the bug fails that test (23 pass / 1 fail); restoring it gives 24/24.

### CRITICAL — the test double was concealing it

`aBridgeDouble` diverged from `transport.ts` in exactly the load-bearing ways: `connect()` always
succeeded, `close()` kept the registry entry, `closed` never resolved, and the factory doubled as the
inspector. `"releases only the session that was detached"` passed **only because** the double was wrong —
against a faithful double, `close()` deletes the entry, so the inspector minted a fresh bridge whose
`wasReleased()` is `false`. The double now refuses a second `connect`, deletes the entry on `close`,
resolves `closed`, and separates factory from inspector.

### Also fixed

| Finding | Resolution |
|---|---|
| Two attachments of one session broke each other | `IpcSessionWire` refcounts attachments; the peer is released on the **last** detach. 4 specs |
| `void held?.close()` unhandled rejection | `.catch` logging via `tddyDebug("tddy:rpc:local-host")` — logged, not swallowed |
| Docs claimed the source is registered *ahead of* LiveKit | Corrected; header now separates provider precedence (ahead — true) from source order (behind) |
| 9 Cypress specs bypassed `ConnectionProviderRegistry` via a local helper | `resolveThrough` deleted; all specs now resolve through the real registry via `useHostConnection` |
| Test title/body mismatch on the source id | Retitled; body asserts `LOCAL_IPC_SOURCE_ID` and says the id is diagnostic, not a merge preference |
| Uncontracted `label.trim() || …` fallback; `openIpcSession` 63 lines | Fallback removed; function down to ~47 lines |

### Reported, not acted on — needs a decision

- **`createLocalHostDirectorySource` is observably redundant in production.** Its descriptor is
  byte-identical to `useServingHostDirectorySource`'s (same `hostId`, same `` `<id> (this daemon)` ``
  label, same `connected`), both are fed the same `appConfig.daemonInstanceId`, and **nothing in
  `packages/tddy-web/src` reads `HostDescriptor.sourceId`** — `daemonHostOf` drops it before any screen
  sees it. Only the source *object* is observable, in `directory.sources`. Nothing deleted; it is named in
  `## Responsibility`, so removing it is a plan change.
- **`liveKitIsConfigured` has no production caller.** Item 5 is enforced by `useCommonRoom`'s own guard, a
  parent's file. The function is this node's named statement of the rule.

### Verification

| Gate | Result |
|---|---|
| `bun run --filter tddy-web test:unit` | **1101 pass, 0 fail** |
| `DesktopIpcHostAcceptance.cy.tsx` | **16 pass, 0 fail** — all now through the real registry |
| `App`, `SessionsDrawerCrossHost`, `DaemonChangeReloadsScreen` | **30 pass, 0 fail** combined |
| `bun run --filter tddy-web build` | ✅ clean |
| `cargo fmt --all --check` | ✅ clean — **0 Rust files changed**, so the Rust CI checks cannot regress |
| `tsc --noEmit` | 522 errors, the pre-existing baseline (down 1); only `TS2307 'bun:test'` in this node's files — **not a repo gate** |

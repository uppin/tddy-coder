# Initial Discovery: Composable React framework (tddy-react-commons)

**Changeset**: [2026-11-07-react-commons-framework.md](./2026-11-07-react-commons-framework.md)
**Date**: 2026-11-07
**Passes**: 2

## Combined Conclusions

**This node (`feature/web-split/react-commons`)** adds the framework package. Facts:

- Framework (developer-supplied, ~4.9k lines in `src/` incl. tests): modules `events`, `stores`,
  `container`, `ports`, `async`, `testkit`; root `index.ts` exports nothing (`export {}`); ports
  re-exports `AppScope` from async; stores re-exports `dequal` as `structurallyEqual`.
- Its tests ran on vitest with `environment: 'jsdom'`, `globals: true`, aliases for each subpath, and
  a setup file setting testing-library's `asyncUtilTimeout` to 5000 ms. **17** test files render
  through `@testing-library/react`; the rest are pure.
- Here: `bun test` has no DOM; `packages/tddy-web/src/test-utils/dom-preload.ts` polyfills only
  `KeyboardEvent`. Developer chose `@happy-dom/global-registrator` in a preload.
- React here is 19 (`tddy-web`); the framework declared React 18 peers.
- Largest files: `async/index.ts` 330, `testkit/index.tsx` 338, `async/app-scope.test.ts` 366.

## Exploration 1: whole-stack discovery (copied in full) — 2026-11-07

**Agent**: parent Grep/Glob/Read (stack planning)
**Scope**: the whole `#web-split` stack; this node's slice is in Combined Conclusions and Exploration 2.

#### Whole-work combined conclusions

**Packages in play.** `packages/tddy-web` (app), `packages/tddy-livekit-web` (LiveKit RPC transport,
3k lines), `packages/tddy-rpc-web` (envelope transport), `packages/tddy-tauri-web` (Tauri IPC transport),
`packages/tddy-connectrpc-testkit` (in-memory ConnectRPC backend), `packages/tddy-desktop` (consumes
the web bundle), root `package.json` (bun workspaces), `.github/workflows/ci.yml` (`web` job,
"Web tests"), `scripts/generated-code.sh` (codegen drift gate, knows `packages/tddy-web`).

**Size of tddy-web (`src/`, non-test, non-story lines).** ~110k total: `gen` 26.4k (generated
protobuf/connect), `components/sessions` 21.4k (+ `prstack` 3.4k, `attachments` 0.7k), `components`
root 3.7k, `rpc` 3.7k + `rpc/connections` 3.1k + `rpc/hostDirectory` 0.5k, `lib` 3.4k,
`components/session` 2.9k, `hosts` 2.7k, `chat` 2.6k, `models` 2.4k, `hooks` 2.0k, `connection` 1.8k,
`accounts` 1.2k, `utils` 1.2k, `projects` 1.0k, `rpc-playground` 0.8k, `test-utils` 0.7k, `routing`
0.7k, `shell` 0.5k, `ui` 0.5k, `settings` 0.4k, `vms` 0.4k, `tasks` 0.4k, `livekit` 0.3k,
`worktrees` 0.3k. Cypress: ~300 component specs in one flat `cypress/component/`, 14 e2e specs,
`cypress/support/` 18.9k lines (page objects, drivers, in-memory RPC adapters, LiveKit fakes).
Only **5** `*.stories.tsx` exist (App, Button, GhosttyTerminal, …); Storybook 9 config in
`packages/tddy-web/.storybook`. The e2e suite runs against a static Storybook build.

**Cross-area import graph (non-test).** `components/sessions` imports nearly everything
(`gen` 163, `lib` 73, `rpc` 69, `ui` 45, `rpc/connections` 28, `chat` 20, `hooks` 20, `utils` 20,
`routing` 14). Cycles to break while extracting:
- `chat → sessions/agentActivityRegistry` (2) — registry moves with chat.
- `hosts → sessions/{Cpu,Disk,Memory}Indicator, hostStatsFormat, ScreenSharingOverlay` — these move
  with hosts.
- `worktrees → sessions/SessionWorktreeTab, worktreeStatsFormat` — move with workspace.
- `prstack → sessions/{CreateSessionDialog, CreateSessionPane, usePresenterLiveKitRoom}` — prstack
  depends on create-session; the presenter-room hook goes behind the LiveKit port.
- `rpc`/`hooks → components/sessions/{CreateSessionProviderField, attachments/*,
  sessionNotificationRegistry, startProgress, worktreeStatsFormat}` — the lower layers reach up; each
  module moves down or to its feature package.
- `models → chat` (2), `routing/selectedHost → gen/web_state_pb, lib/participantRole`,
  `lib/{liveKitRoomsState, sessionIndicator, sessionReadsLocalPaths, terminalHistoryLoader} → gen`,
  `lib/sessionReadsLocalPaths → rpc/daemonTransportFlavour`.

**LiveKit reach.** 31 non-test files import `livekit-client` or `tddy-livekit-web` directly:
`index.tsx`; `components/ParticipantVideoPreviewDialog.tsx`; `chat/{useAgentChat, AgentChat,
useAcpSession}`; `sessions/{SessionScreenSharingTab, useScreenSharingInput, SessionVncTab,
SessionUsageTab, VncOverlay, sessionMainPaneProps, useSessionsDrawerSessionList,
SessionInspectorDrawer, useSessionUsage, usePresenterLiveKitRoom, ScreenSharingOverlay}`;
`sessions/prstack/PrStackChat`; `hooks/{participantCameraVideo, useRoomParticipants,
useCommonRoom}`; `session/useCodeSession`; `lib/liveKitConnectionErrors`; `rpc/{selectedDaemon,
transportProvider}`; `rpc/connections/{liveKit, livekit/roomTerminalFeed, livekit/sessionConnection,
livekit/useDirectRoomTerminal}`; `rpc/hostDirectory/{presenceRoom, useHostPresence, liveKitSource}`.
After the split only the LiveKit packages may import it.

**RPC seam today.** `rpc/transportProvider.tsx` — `RpcTransportProvider({httpTransport,
liveKitFactory})`; Cypress `cy.mountWithRpc(jsx, backend)` (`cypress/support/rpc/inMemory.tsx`)
injects one in-memory transport (`tddy-connectrpc-testkit`) for both HTTP and LiveKit. That is the
shape the `ClientsPort` generalises: components ask for a transport for daemon / host / session; the
wiring supplies HTTP, Tauri or LiveKit implementations.

**Module resolution.** Workspace packages are resolved **from source** through aliases in
`packages/tddy-web/vite.config.ts` (`tddy-livekit-web`, `tddy-connectrpc-testkit`) and
`tsconfig.json` `paths`, because their `exports` point at gitignored `dist/`. `@/*` →
`./src/*` is used by shadcn code (`@/components/ui` 11, `@/lib/utils` 7, `@/routing/*` 2). React is
pinned to tddy-web's copy via `dedupe` + alias (two Reacts in Cypress CT otherwise). React Compiler
is on except under Cypress (`CYPRESS_DISABLE_REACT_COMPILER=1`). React 19, Storybook 9, Cypress 14,
Vite 6, bun test for unit tests.

**CI today.** `ci.yml` `web` job "Web tests": needs `rust-build` (downloads `rust-fixture-bins` for
tddy-livekit-web's echo server), installs nix, Cypress libs, `bun install --frozen-lockfile`,
`bun run build`, unit tests (`tddy-web test:unit:ci`), transport packages (`tddy-rpc-web`,
`tddy-tauri-web` `test:ci`), pulls LiveKit image, component tests (`tddy-web`, `tddy-livekit-web`
`cypress:component:ci`), publishes a JUnit check named "Web tests". `build-gate.yml` decides draft
skips. **The `master` ruleset requires `Rust tests`, `Web tests`, `Rust lint`, `Rust build`.**
Decision (developer): per-package checks replace `Web tests`; the agent edits the ruleset when n1
lands — not before, because every open PR would lack the new checks.

**The framework to import** (developer-supplied, ~6.2k lines incl. tests and examples): modules
`events` (typed component events, adapters, observers, `withEvents`), `stores` (rxjs-backed
zustand-style stores, `useStore`), `container` (app container, contexts, slice hooks, React-free
handlers), `ports` (port declarations), `async` (request, poller with refresh/visibility/equality,
command errors, app-scope resource factories), `testkit` (container testkit, async doubles,
app-scope double), plus an `examples/editor` app. Deps `rxjs`, `dequal`; peer React 18; tests on
vitest + jsdom + testing-library; Storybook 8 stories in examples. Here: React 19, bun test, the
local private npm registry (`bun run local-registry-install`; no public registry). **The source is
never named in PRs, docs or code.**

**Existing records in the path.**
- `packages/tddy-web/docs/code-issues/`: `oversized-file-{create-session-pane,
  ghostty-terminal-session, ghostty-terminal, pr-stack-screen, session-main-pane,
  use-model-registry-fan-out}.md` — move with their files (records are written directly, no
  changeset needed); a move does not fix them.
- `docs/dev/todo/`: 57 open entries mention tddy-web/cypress/livekit/storybook/CI. Relevant:
  `2026-10-05-ci-script-tests-are-not-run-in-ci.md` (n1 resolves: the affected-package script's tests
  and the other CI script tests run in a workflow); `2026-10-18-web-tests-start-livekit-from-the-floating-master-image.md`
  (⚠ during n1/n4 — the LiveKit image pull moves into the livekit-rpc package's workflow; reading
  `.config/livekit-server.image` is a cheap fix to take there); `2026-10-23-cypress-runs-without-the-react-compiler.md`
  (⚠ during — every new package's Cypress config must keep the same switch, not silently diverge);
  `2026-09-09-versioned-proto-package-names.md` (— unrelated: n5 moves generated code, does not
  rename protos); `2026-09-06-…optional-livekit-stack-evaluation.md` (ℹ — `liveKitIsConfigured` in
  `rpc/connections/localHost.ts` moves behind the LiveKit port in n6).
- Active `docs/dev/1-WIP/` changesets touching tddy-web: `2026-09-09-livekit-rooms-panel-every-connection.md`,
  `2026-09-20-sandboxed-codebase-managed-workflow.md`, `2026-10-09-terminal-manual-history-toggle.md`
  — stale-looking WIP; any file they name moves package under this stack.

### Pass 1: framework source — 2026-11-07

**Agent**: parent Grep/Glob/Read
**Scope**: the composable framework to import.

- `find <framework> -type f` → `src/{async,stores,testkit,container,events,ports}/…` with tests,
  `examples/editor/*`, `docs/changesets/*` (7), `README.md`, `AGENTS.md`, `vitest.config.mts`,
  `vitest.setup.ts`, `tsconfig*.json`, `.eslintrc.json`.
- `package.json`: ESM, subpath exports `.`, `./events`, `./stores`, `./container`, `./ports`,
  `./async`, `./testkit`; deps `dequal ^2.0.3`, `rxjs ^7.8.2`; peer `react/react-dom ^18.2.0`; dev
  `vitest ^4`, `jsdom`, `@testing-library/react ^13`, `observable-hooks`, Storybook 8.
- Companion storybook package: Storybook 8 react-vite, depends on component packages, `src/Introduction.mdx`.
- `wc -l` over `*.ts*`: 6177 lines.

### Pass 2: tddy-web shape — 2026-11-07

**Agent**: parent Grep/Glob/Read
**Scope**: package.json, src tree sizes, component dirs, cypress tree, workspace packages, CI jobs.

- `packages/tddy-web/package.json`: scripts `build` (prebuild builds tddy-livekit-web), `test:unit`
  (bun test over listed dirs), `cypress:component[:ci]`, `cypress:e2e` (Storybook static + http-server),
  `storybook`; deps include `livekit-client`, `ghostty-web`, `radix-ui`, `tailwindcss`, `tddy-tauri-web`;
  dev `tddy-livekit-web`, `tddy-rpc-web`, `tddy-connectrpc-testkit`, `livekit-server-sdk`, Storybook 9, Cypress 14.
- Per-dir line counts (see Combined Conclusions). `ls cypress/component` → ~300 specs (flat).
- Root `package.json` workspaces: tddy-web, tddy-livekit-web, tddy-rpc-web, tddy-tauri-web,
  tddy-desktop, tddy-rust-typescript-tests, tddy-connectrpc-testkit.
- `.github/workflows/`: `automerge.yml`, `build-gate.yml`, `ci.yml` (917 lines), `forcebuild.yml`, `vm-tests.yml`.

### Pass 3: import graph — 2026-11-07

**Agent**: parent script (`/tmp` python resolving relative imports between areas)
**Scope**: area→area edges in tddy-web `src/`, non-test, non-story.

Full edge list recorded at planning time; the load-bearing edges and cycles are in Combined
Conclusions. Targeted greps:
- `grep -rlE "from ['\"](livekit-client|tddy-livekit-web)"` → the 31 files listed above.
- `hosts → ../sessions/*`: CpuCoresIndicator, DiskSpaceIndicator, MemoryIndicator, ScreenSharingOverlay, hostStatsFormat.
- `chat → ../sessions/agentActivityRegistry` (2). `worktrees → ../sessions/{SessionWorktreeTab, worktreeStatsFormat}`.
- `rpc|hooks → components/sessions/*`: CreateSessionProviderField, attachments/HostDocumentPicker,
  attachments/pendingAttachment, sessionNotificationRegistry, startProgress, worktreeStatsFormat.
- `prstack → ../{CreateSessionDialog, CreateSessionPane, usePresenterLiveKitRoom}`.
- `@/` alias users: `@/components/ui` 11, `@/lib/utils` 7, `@/routing/*` 2.
- `routing/selectedHost.ts → gen/web_state_pb, lib/participantRole`; `lib/*` gen importers listed above.

### Pass 4: CI, ruleset, build config — 2026-11-07

**Agent**: parent Grep/Read + `gh api`
**Scope**: `ci.yml` `web` job (lines 793–917), `build-gate.yml`, ruleset, `vite.config.ts`, `tsconfig.json`.

- `web` job: `needs: rust-build`; steps as in Combined Conclusions; LiveKit image
  `livekit/livekit-server:master` pulled in CI and named in `livekitDockerTestkit.ts`.
- `build-gate.yml`: reusable (`workflow_call`), output `build`; drafts skip unless `#forcebuild`.
- `gh api repos/uppin/tddy-coder/rulesets/<id>` → required checks `Rust tests`, `Web tests`, `Rust lint`, `Rust build` (integration 15368), non-strict.
- `vite.config.ts`: tailwind plugin, React Compiler switch, dedupe + React alias, `@` alias, source
  aliases for `tddy-livekit-web`/`tddy-connectrpc-testkit`, `/rpc` + `/api` proxy, `fs.allow` repo root.
- `tsconfig.json`: bundler resolution, `paths` `@/*`, `tddy-livekit-web` → source.
- `scripts/generated-code.sh`: per-package `buf generate` drift gate; `PACKAGES` list derived from args.

### Pass 5: records and test support — 2026-11-07

**Agent**: parent Grep/Read
**Scope**: `docs/dev/todo/`, `packages/tddy-web/docs/code-issues/`, `docs/dev/1-WIP/`, Cypress support.

- `grep -rlE "tddy-web|tddy-livekit-web|livekit-client|cypress|storybook|bun workspace|ci\.yml" docs/dev/todo/ | xargs grep -L 'Status:\*\* Resolved'` → 57 entries; bodies read for the five classified above.
- `ls packages/tddy-web/docs/code-issues` → six oversized-file records.
- `cypress/support/` subdirs: `drivers/` (8), `fixtures/`, `livekit/` (`commonRoomConnection`,
  `fakeCommonRoom`), `pages/` (page objects per screen), `rpc/` (`inMemory.tsx` — `mountWithRpc`,
  `recordingLiveKitRpc`, `vncBackend`, `usageBackend`, `withLiveCommonRoom`,
  `screenSharingBackend`, `withSelectedDaemon`), `util/`, `testIds.ts`, `livekitDockerTestkit.ts`.
- `cypress/support/rpc/inMemory.tsx`: `mountWithRpc(component, backend)` mounts inside
  `RpcTransportProvider httpTransport={t} liveKitFactory={() => t}`.
- `tddy-connectrpc-testkit/src/index.ts`: `anInMemoryRpcBackend`, `InMemoryRpcBackend`,
  `registerServerStreamFallback`.

## Exploration 2: framework exports and test environment — 2026-11-07

**Agent**: parent Grep/Read
**Scope**: the public surface per subpath and what its tests need from the environment.

- `grep -E "^export" src/*/index.ts*` → the export lists recorded in the changeset's `## Responsibility`.
- Read the vitest config (jsdom, globals, subpath aliases) and setup (`configure({ asyncUtilTimeout: 5000 })`).
- `grep -l "render\|@testing-library" src/*/*.test.ts*` → 17 files.
- `wc -l src/*/*.ts*` → 4881 lines.
- Read `packages/tddy-web/src/test-utils/dom-preload.ts` and `bunfig.toml` (`[test] root = "src"`, `preload`).
- `grep happy-dom|jsdom|testing-library bun.lock` → only `@testing-library/{dom,jest-dom,user-event}` (transitive via Storybook).

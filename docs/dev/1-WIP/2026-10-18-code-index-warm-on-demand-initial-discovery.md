# Initial Discovery: Code index warms on demand, with live LSP progress

**Changeset**: [2026-10-18-code-index-warm-on-demand.md](./2026-10-18-code-index-warm-on-demand.md)
**Date**: 2026-10-18
**Passes**: 2

## Combined Conclusions

- **LSP progress is already consumed.** rust-analyzer is launched with `window.workDoneProgress` and
  `experimental.serverStatusNotification`; `tddy-lsp` acks `window/workDoneProgress/create` and fans every
  notification into `NotificationSink` (broadcast, no replay). `ServerChatter`
  (`tddy-code-restructuring/src/backends/rust/chatter.rs`) folds `$/progress` + `serverStatus`;
  `tddy-index-daemon/src/warm.rs` streams `IndexProgress{line, phase, percentage, furthest, ready}` per distinct
  line, unthrottled, until the root's `GraphLoad` latch (`graph.rs`) sets on `quiescent`.
- **The only warm trigger is session start** (`IndexWarmupObserver` ← `announce_worktree_ready`, workspace +
  non-sandboxed claude/cursor starts). Its output lands in `SessionIndexProgress`, an in-memory, session-keyed
  record that `code_navigation.WatchCodeIndex(session)` follows. Restart, sandboxed, tool and split sessions → no
  record → the stream ends empty → the strip reads *not warmed*.
- **Opening the Code pane only watches; it never warms.** `WorktreeCodePane` has `projectId` + `worktreePath`
  but `CodeIndexStatusStrip` is given only `sessionId`.
- **Navigation and MCP tools start rust-analyzer silently.** Any `WorkspaceIndex::client_for` spawns the server
  and attaches the `GraphLoad` latch, but publishes progress nowhere unless a `Warm` stream is open. MCP `Lsp*`
  and `restructure_*` tools go `ExecuteTool` → `execute_tool_with_env(worktree_root, …, session_id)` →
  process-global `IndexLspExecutor` → `code_index` directly; `session_id` is dropped at `tool_lsp`.
- **A second `Warm` joining mid-load starts cold**: a fresh `ServerChatter`, no replay of `begin` titles, so
  phases read `"working"` until a new `begin`. A per-root, always-on fold is missing.
- **Navigation answers empty while loading**: `navigation.rs` never consults `GraphLoad`; no proto carries an
  index state. The web shows "No definition found".
- **Hover card** — `WorktreeCodePane` hover result renders as an in-flow panel that grows with content (user
  screenshot 2026-10-18 08.35); markdown headings merge into the next paragraph.
- Authorisation: navigation uses `resolve_listed_worktree(token, project, worktree)`; the watch uses
  `resolve_owned_session_dir(token, session)`.

- **Bidi is carried on every wire that reaches a host.** Host clients come from `useHostClient`
  (`rpc/connections/registry.tsx:233`) — LiveKit or the desktop IPC bridge (`tddy-tauri-web/src/transport.ts`,
  "a duplex frame pipe"). Plain `connect-web` HTTP serves only the page's own control plane. Precedent bidi RPCs:
  `AcpService.Session`, `StreamTerminalIO`, `sandbox.SessionChannel`. `anInMemoryRpcBackend().implement(...)`
  serves bidi handlers (`AgentChatAcpStreamingAcceptance.cy.tsx`).
- **Hover today**: `CodeBlock.tsx` wraps every identifier in a span; `HOVER_DWELL_MS = 250` then `onHover`;
  ctrl/meta-click navigates. `HoverCard` (`CodeNavigationOverlays.tsx`) renders `hover.markdown` as a raw `<pre>`
  inside an `absolute inset-x-3 bottom-3` stack — no max height, no anchoring, no markdown rendering.
  `renderSimpleMarkdown.tsx` exists. No identifier is ever underlined.
- **Step 2b** — `tddy-index-daemon/src/index.rs` 522 lines, `Restructure: required`, unclaimed → put the per-root
  fold in `graph.rs`, not `index.rs`. `warm.rs::narrate_until_loaded` nesting 5 → collapses once Warm forwards a
  per-root watch. `SessionMainPane.tsx` 674/500 (regressed by #652) → do not touch. `tddy-tool-engine/src/lib.rs`
  816/500 → MCP trigger must not land there. `tddy-daemon/src/runtime.rs` 1,777 → wiring net ≤ 0.
  No claimed issue in path. `tddy-lsp`, `tddy-lsp-executor` have no `code-issues/` (unanalyzed).

## Exploration 1 — Explore agent: warm trigger, progress path, MCP route, tests, docs

# Discovery: code-index warming and progress (tddy-coder at `515c00149`)

**Short answer.** The index daemon already reads rust-analyzer's own `$/progress` and `experimental/serverStatus` notifications and streams them through `Warm`, and the Code pane strip already shows phase, percentage and line live. What's missing is the trigger you want:
- Warming starts in only one place: the session-start observer. It covers only non-sandboxed claude-cli, cursor-cli and workspace starts.
- Opening the Code pane only *watches* a warm. It never starts one.
- An MCP code tool reaches the index without the daemon noting which session asked, so it never records progress.

Notes on scope:
- The `chatter.rs` you named is not in tddy-index-daemon. It lives at `/Users/mantasi/Code/tddy-coder/packages/tddy-code-restructuring/src/backends/rust/chatter.rs`.
- `readiness-and-gates.md` is at `/Users/mantasi/Code/tddy-coder/packages/tddy-code-restructuring/docs/readiness-and-gates.md`.
- The repo has many copies under `.claude/worktrees/*` and `.worktrees/*`. I ignored them and read only `packages/`.

---

## 1. Sequence

1. Ran `git log -5` and `ls packages`, and searched by filename for `code_navigation*.proto`, `code_index*.proto`, `code_navigation_pb*`, `CodeIndexStatusStrip*` and `useCodeIndexProgress*`, to locate the main files.
2. Grepped `WatchCodeIndex|watchCodeIndex` across `packages` (rs/ts/tsx/proto/md, excluding `_pb.ts`) and listed matching files under `docs/`, to map everyone who produces or consumes the stream.
3. Read `packages/tddy-service/proto/code_navigation.proto` in full (the web-facing contract).
4. Read `packages/tddy-index-daemon/proto/code_index.proto` in full (the index contract).
5. Read `useCodeIndexProgress.ts`, `CodeIndexStatusStrip.tsx` and `SessionIndexingIndicator.tsx` (the web consumers).
6. Grepped `CodeIndexStatusStrip|SessionIndexingIndicator|CodeNavigationService|codeNavigationClient` in `tddy-web/src`, to find where they mount.
7. Read `WorktreeCodePane.tsx`, `SessionMainPane.tsx` (lines 480–520 and 590–640) and `SessionsDrawerScreen.tsx` (495–515), for mount points and client creation.
8. Read `codeNavigationApi.ts`, to see how Definition, References and Hover are called from the web.
9. Read `tddy-daemon-rpc/src/code_index_warmup.rs` and `code_navigation.rs` (lines 1–420), the daemon-side implementation.
10. Read `tddy-daemon/src/runtime.rs` (lines 1120–1200 and 1420–1445) and grepped `IndexWarmupObserver|SessionWorktreeObserver|worktree_ready|SessionIndexProgress|with_index_progress|warm_for_session`, for the wiring.
11. Grepped `resolve_owned_session_dir|resolve_listed_worktree|session_dir_for` in `tddy-worktree-service/src`, for authorisation.
12. Read `tddy-agent-launch/src/session_worktree_observer.rs` and `svc_start_session_core.rs` (lines 320–430), for where session start announces the worktree.
13. Listed the tddy-index-daemon tree and line counts. There is no `backends/rust` there.
14. Searched for `chatter*` and grepped `$/progress|WorkDoneProgress|workDoneProgress|ProgressParams|cachePriming|Roots Scanned`, to find who consumes LSP progress.
15. Read `tddy-index-daemon/src/warm.rs` and `graph.rs` in full.
16. Read `tddy-code-restructuring/src/backends/rust/chatter.rs` (lines 1–420), the progress fold.
17. Read `tddy-lsp/src/client/notifications.rs` and grepped tddy-lsp for `workDoneProgress|server_status|capabilities`.
18. Read `tddy-lsp/src/client/dispatch.rs` (50–110) and `client.rs` (75–200), and grepped `workDoneProgress|serverStatusNotification|with_capabilities` across the repo.
19. Read `tddy-code-restructuring/src/backends/rust.rs` (210–240) and `tddy-index-daemon/src/main.rs` (195–218), for the capabilities advertised.
20. Read `tddy-index-daemon/src/index.rs` (80–140 and 260–430): `WorkspaceIndex`, `client_for`, `graph_load_of`, `record_use`.
21. Read `tddy-index-daemon/src/navigation.rs` (1–140).
22. Grepped `TDDY_INDEX_SOCKET|index_socket|CodeIndexServiceClient` and LSP tool names, to find the MCP tools.
23. Read `tddy-lsp-executor/src/index_backed.rs` (1–200 and 370–420) and `lsp_tools.rs` (1–80).
24. Grepped `IndexLspExecutor|IndexChannel for|select_lsp_executor`, and read `runtime.rs` (1225–1290), for the executor wiring.
25. Read `tddy-toolcall/src/toolcall/lsp.rs` (the `LspExecutor` trait) and grepped the `lsp_executor()` call sites.
26. Read `tddy-tool-engine/src/lib.rs` (240–330) and `restructure_tools.rs`, for MCP dispatch.
27. Grepped `execute_tool_with_env|execute_tool(` call sites, and read `tddy-tools/src/index_client.rs` and `restructure_tools.rs` headers and `RESTRUCTURE_TOOL_NAMES`.
28. Read `tddy-session-lifecycle/.../local_exec_tools.rs` (150–200 and 320–365) and `tddy-daemon-sandbox/src/conversation_tool.rs` (25–60), for the daemon's ExecuteTool route.
29. Read `tddy-agent-launch/src/jail_env_builders.rs` (55–95) and `tddy-sandbox-app/src/spawn.rs` (675–700), for how LSP and restructure tools are gated.
30. Read all three todo docs.
31. Read `tddy-web/docs/session-start-and-indexing-progress.md` and grepped `docs/ft/web/session-code-pane.md`.
32. Grepped `code-index-service.md` and read lines 55–130; found `readiness-and-gates.md`.
33. Grepped and read `readiness-and-gates.md` (25–70) and the changeset `2026-10-18-warm-names-the-current-phase.md`. Grepped rust-analyzer phase titles repo-wide.
34. Read `cypress/component/CodeIndexStatusStrip.cy.tsx`, the page object, `cypress/support/rpc/inMemory.tsx`, and the `CodeNavigationService` fakes in other specs.
35. Read the fake in `SessionStartAndIndexingProgress.cy.tsx` (180–240).
36. Listed test functions in `tddy-daemon/tests/code_index_warmup_acceptance.rs` and read its header.
37. Listed the warm and navigation tests in `code_index_service_acceptance.rs` and read `warming_a_root_forwards_…`.
38. Grepped `tddy-lsp/tests/bin/fake_lsp.rs` for its progress modes and read the fake-host builder in the acceptance test.
39. Read `tddy-daemon-rpc/docs/architecture.md` (25–40), `tddy-daemon/docs/code-navigation-service.md` (60–120), and grepped the warm dispatch in `tddy-index-daemon/src/service.rs`.
40. Grepped codegen config, `testIds.ts`, and read `docs/dev/changesets/2026-10-18-code-pane-index-status-strip.md`.
41. Grepped `TODO(indexing-indicators)` and the idle timeout in index-daemon `main.rs`.

---

## 2. Inspected files (with the excerpts that mattered)

### Web
**`/Users/mantasi/Code/tddy-coder/packages/tddy-web/src/components/session/useCodeIndexProgress.ts`**
- Lines 17–22: the state type is `waiting | progress{progress: CodeIndexProgress} | ready | failed{reason} | untracked`.
- Lines 25–61: `useEffect` keyed on `[client, sessionToken, sessionId]`. It runs `for await (const progress of client.watchCodeIndex({ sessionToken, sessionId }))`:
  - `ready` → `ready`
  - `error !== ""` → `failed`
  - anything else → `progress`
  - stream ends having heard nothing → `untracked`
  - exception → `failed`
- It cancels with a `cancelled` flag rather than an AbortSignal, because the LiveKit transport ignores the signal.

**`.../session/CodeIndexStatusStrip.tsx`**
- Props: `client`, `sessionToken`, `sessionId`.
- Test ids: `worktree-code-index-status` (carries `data-state`), `worktree-code-index-status-summary` and `worktree-code-index-status-detail`.
- `describe()` (lines 58–78):
  - progress → `Indexing — ${phase} ${percentage}%`, with `line` as the detail
  - untracked → "Code index not warmed for this session", detail "it loads on the first definition or hover…"

**`.../session/SessionIndexingIndicator.tsx`**
- Renders only in the `progress` and `failed` states (`data-testid="session-indexing-indicator"`).

**`.../session/WorktreeCodePane.tsx`**
- Lines 137–143: `{navigationClient && sessionId && (<CodeIndexStatusStrip client={navigationClient} sessionToken={sessionToken} sessionId={sessionId} />)}`.
- The pane also holds `projectId` and `worktreePath`, but the strip gets neither, so watching is keyed by session only.
- Navigation is offered only for `.rs` files (line 75).

**`.../sessions/SessionMainPane.tsx`**
- Lines 498–503 mount the header indicator.
- Lines 605–624 mount `WorktreeCodePane`, only when `codeOpen && codePaneEnabled && worktreeClient`, passing `navigationClient={codeNavigationClient}` and `sessionId={selectedSession.sessionId}`. This mount is the natural "Code pane opened" moment.

**`.../sessions/SessionsDrawerScreen.tsx:507`**
- `const activeCodeNavigationClient = useDaemonClientFor(CodeNavigationService, selectedOwningHost);`, passed down at line 937.

**`.../session/codeNavigationApi.ts`**
- Lines 46–70: `definition`, `references` and `hover` send `{sessionToken, projectId, worktreePath, relPath, position}`.

**Generated TypeScript**
- `/Users/mantasi/Code/tddy-coder/packages/tddy-web/src/gen/code_navigation_pb.ts`, from `packages/tddy-web/buf.gen.yaml` (`out: src/gen`).
- A mirror at `packages/tddy-rust-typescript-tests/gen/code_navigation_pb.ts`.
- Rust codegen: `packages/tddy-service/build.rs:210,639`.

### Protos
**`/Users/mantasi/Code/tddy-coder/packages/tddy-service/proto/code_navigation.proto`** (package `code_navigation`, service `CodeNavigationService`)
- RPCs: `Definition`, `References`, `Hover`, `WatchCodeIndex(WatchCodeIndexRequest) returns (stream CodeIndexProgress)` (line 35), `OpenPlan`, `WatchPlan` (stream), `RunPlan` (stream).
- The comment on `WatchCodeIndex` reads: "Once a session's worktree exists the daemon starts `code_index.Warm` … This stream opens with that latest progress …, follows it, and ends after the message carrying `ready` or `error`. A session nothing warmed … ends with no message."
- `WatchCodeIndexRequest { session_token = 1; session_id = 2; }`.
- `CodeIndexProgress { line=1; phase=2 (e.g. "Indexing" or "Loading"); percentage=3 (uint32); furthest=4 (string); ready=5; error=6 }`.
- Definition, References and Hover requests: `{session_token, project_id, worktree_path, rel_path, SourcePosition position}`.
- `HoverResponse { optional string markdown }`.
- `CodeLocation { rel_path, SourceRange range, outside_worktree }`.
- None of the navigation responses has an index-state field.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/proto/code_index.proto`** (package `code_index`, service `CodeIndexService`)
- Line 24: `rpc Warm(WarmRequest) returns (stream IndexProgress);`, documented "Idempotent: a root that is already warm completes immediately."
- `WarmRequest { workspace_root = 1; }`.
- Lines 100–112:
  ```proto
  // `phase` and `percentage` come from the language server's own `$/progress`; `furthest` is the
  // highest percentage seen in any phase ...
  message IndexProgress { string line = 1; string phase = 2; uint32 percentage = 3; string furthest = 4; bool ready = 5; }
  ```
- `RestructureEvent.oneof { IndexProgress indexing = 1; … }`: Check and Apply also carry progress.
- `Workspaces` → `WarmWorkspace { workspace_root, ready, idle_seconds }`.
- Navigation: `Definition`, `References`, `Hover`, `Symbols`, `Diagnostics`, each `{workspace_root, file, position}`.
- No structured per-token or multi-phase fields; one current phase per message.

### Daemon side
**`/Users/mantasi/Code/tddy-coder/packages/tddy-daemon-rpc/src/code_index_warmup.rs`**
- `SessionIndexProgress` (lines 32–79): `Arc<Mutex<HashMap<String, watch::Sender<Option<CodeIndexProgress>>>>>`.
  - `record` creates an entry or replaces its value.
  - `follow` subscribes but never creates an entry.
- `warm_for_session(index_daemon: Option<&Arc<dyn IndexChannelSource>>, progress, session_id, worktree) -> Option<JoinHandle<()>>` (lines 88–123):
  - Returns `None` if there is no index daemon or no `worktree/Cargo.toml`.
  - Records a synthetic `{line:"Starting the code index", phase:"Starting"}` before spawning.
  - On error, records `{error: reason}`.
- `warm()` (lines 127–156) dials `source.connect()`, calls `CodeIndexServiceClient::warm`, and runs `progress.record(session_id, web_progress(update))` for every message. A stream that ends without `ready` is an Err.
- `IndexWarmupObserver` implements `SessionWorktreeObserver::worktree_ready`. The warm is detached and never deduplicated: each call opens a new Warm stream.

**`.../tddy-daemon-rpc/src/code_navigation.rs`**
- `IndexChannelSource` trait (lines 49–53): `async fn connect(&self) -> Result<Channel, IndexChannelError>`.
- `CodeNavigationServiceImpl { worktrees: Arc<WorktreeServiceImpl>, index_daemon: Option<Arc<dyn IndexChannelSource>>, index_progress: SessionIndexProgress }`.
- `authorise_and_connect` (lines 93–121) runs `resolve_listed_worktree(token, project, worktree_path)`, then `validate_rel_path_shape`, then requires the index daemon (otherwise `failed_precondition`), then `connect` (otherwise `unavailable`).
- `definition`, `references` and `hover` (lines 139–226) forward 1:1, with `workspace_root = root.display()`.
- `watch_code_index` (lines 230–266):
  - Authorises with `resolve_owned_session_dir(token, session_id)`.
  - With no entry from `follow`, returns an empty stream.
  - Otherwise spawns a loop: `borrow_and_update` → send → stop when `ready || !error.is_empty()`.
  - `WATCH_BUFFER = 8`. It never triggers a warm.
- `build_code_navigation_entry` registers under `"code_navigation.CodeNavigationService"`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-daemon/src/runtime.rs`**
- Lines 1131–1142: `IndexDaemonRegistry::new(IndexDaemonSpawn::from_config(..))`. The registry is always created; an absent config section means defaults.
- Lines 1147–1148: one shared `SessionIndexProgress`.
- Lines 1188–1199: `connection_impl.with_worktree_observer(IndexWarmupObserver::new(registry, session_index_progress.clone()))`.
- Lines 1426–1436: `CodeNavigationServiceImpl::new(worktree_service_impl, registry).with_index_progress(session_index_progress.clone())`.
- Lines 1236–1262: `select_lsp_executor(index_channel, TddyLspExecutor)` and `register_restructure_executor(IndexRestructureExecutor::new(index_channel))`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-worktree-service/src/service.rs`**
- `resolve_owned_session_dir` (lines 162–173): token → OS user → `<sessions base>/sessions/<id>` must be a directory, otherwise `not_found`.
- `resolve_listed_worktree` (lines 212–235): token → main repo of the project → path must appear in `git worktree list`, otherwise `failed_precondition`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-agent-launch/src/session_worktree_observer.rs`**
- `trait SessionWorktreeObserver { fn worktree_ready(&self, session_id: &str, worktree: &Path); }`.
- `announce_worktree_ready` resolves the worktree from `.session.yaml` using `resolve_worktree_root_for_session`.

**`.../tddy-agent-launch/src/svc_start_session_core.rs`**
- `announce_worktree_ready` is called at lines 335 (workspace), 386 (claude-cli, non-sandboxed) and 424 (cursor-cli, non-sandboxed). Those are the only call sites.
- Line 59 has `TODO(indexing-indicators)`.
- Sandboxed starts return early (lines 359–368 and 400–404) without announcing.

### Index daemon
**`/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/src/warm.rs`**
- `serve_warm` (lines 33–88):
  1. Sends `loading(root)` (`"loading the crate graph at …"`, empty phase).
  2. `index.client_for(&root)` (which spawns rust-analyzer if needed).
  3. `index.graph_load_of(&root)`.
  4. `client.subscribe_notifications()`.
  5. `narrate_until_loaded`.
  6. On `Loaded`, sends `ready(root)`; on `ServerGone`, `unavailable`; on `CallerGone`, cancels.
- `narrate_until_loaded` (lines 102–145) uses `ServerChatter::unthrottled()` and selects over `graph.loaded()`, `events.closed()` and `notifications.recv()`:
  - `Received` → `chatter.absorb`
  - `Lost(n)` → `lost_sight_of`
  - `Ended` → loaded or gone
- `phase_of` (lines 172–181):
  ```rust
  let (percentage, phase) = chatter.current().unwrap_or((0, ""));
  IndexProgress { line, phase, percentage: percentage as u32, furthest: chatter.how_far(), ready: false }
  ```
- Unit tests: `carries_the_furthest_the_load_reached_…`, `names_the_phase_running_now_…` (Roots Scanned 100% then Indexing 37), `says_how_many_progress_notifications_a_wait_lost_sight_of`.

**`.../tddy-index-daemon/src/graph.rs`**
- `GraphLoad { loaded: watch::Receiver<bool> }`.
- `GraphLoad::watching(client)` spawns `fold_until_the_graph_is_loaded`, which folds through `ServerChatter::default()` until `chatter.quiescent()` and then latches `true` permanently.
- Also `is_loaded()`, `still_watching()` and `loaded().await`, which returns false when the server is gone.

**`.../tddy-index-daemon/src/index.rs`**
- `WorkspaceIndex { servers: LspRegistry, roots: Arc<Mutex<BTreeMap<PathBuf, RootState>>>, … }`, where `RootState` holds `graph: Option<GraphLoad>` and `tree`.
- `client_for` (lines 276–322):
  1. `read_tree` → `servers.get_or_spawn(LspKey{root, Rust})` → `set_request_timeout`.
  2. `record_use`, which attaches `GraphLoad::watching` on the first use of each server (lines 407–426).
  3. Sends `didChangeWatchedFiles` for any changes and re-resolves plans.
- `graph_load_of` (line 389).
- `warmth_of` (`AlreadyWarm`/`NotYetIndexed`).
- `warm_workspaces` (always `ready: true`; it means "server alive", not "graph loaded").

**`.../tddy-index-daemon/src/navigation.rs`**
- `serve_definition`, `serve_references` and `serve_hover` go through `Activity::arrived` → `asked_document` → `synced_document(index, …)`, which calls `client_for`, then the LSP query.
- They never consult `GraphLoad`. While loading, they answer empty.

**`.../tddy-index-daemon/src/service.rs:79–91`**
- `type WarmStream = EventStream<IndexProgress>`, and `warm()` delegates to `crate::warm::serve_warm`.

**`.../tddy-index-daemon/src/main.rs`**
- Lines 207–218: rust-analyzer is launched with `client_capabilities()` and `server_settings()`.
- Line 60: `A_SERVER_MAY_SIT_UNUSED_FOR = 600s`.

### tddy-lsp and the chatter fold
**`/Users/mantasi/Code/tddy-coder/packages/tddy-code-restructuring/src/backends/rust/chatter.rs`**
- `pub struct ServerChatter` (lines 38–100) keeps, all per token where relevant: `titles` (title by token, from `begin`), `last`, `shown`/`shown_at` (throttle), `unthrottled`, `quiescent`, `reported_status`, `health`, `furthest: Option<(u64, String)>`, `current: Option<(u64, String)>`, `reached_by_token`, `changed_at`.
- `absorb_at` (lines 123–149) handles `"$/progress"` and `"experimental/serverStatus"` (health, quiescent).
- `progress_at` (lines 164–216):
  - `begin` stores the title.
  - `end` removes the token's state and returns `None`.
  - Line = `progress_line(title, value)`, which formats as `"{title}: {message} ({pct}%)"`.
  - `reached()` updates `furthest`; `reported_on()` sets `current = (max pct for the token, phase)`.
  - Deduplicates and throttles with `PROGRESS_INTERVAL = 2s` unless unthrottled.
- Public readers: `how_far()` (`"{last}; furthest {phase} {pct}%"`), `quiescent()`, `loading()`, `degraded()`, `quiet_for()`, `furthest()`, `current()`.
- `UNTITLED_PHASE = "working"`: a report whose `begin` wasn't seen gets the phase "working".

**`/Users/mantasi/Code/tddy-coder/packages/tddy-lsp/src/client/notifications.rs`**
- `NotificationSink { kept: VecDeque (256), live: broadcast::Sender<Value> (256), status: latest experimental/serverStatus }`.
- `NotificationEvent::{Received(Value), Lost(u64), Ended}`.
- A subscriber gets only notifications that arrive after it subscribes. There is no replay; only the latest server status is retained.

**`.../tddy-lsp/src/client/dispatch.rs:62–74`**
- A server→client request (including `window/workDoneProgress/create`) gets a `null` reply.
- Every other notification goes to `notifications.record(...)`.

**`.../tddy-code-restructuring/src/backends/rust.rs:231–232`**
- `"window": { "workDoneProgress": true }, "experimental": { "snippetTextEdit": false, "serverStatusNotification": true }`.

### MCP tools
**`/Users/mantasi/Code/tddy-coder/packages/tddy-lsp-executor/src/index_backed.rs`**
- `IndexChannel` trait: `async fn connect(&self) -> Result<Channel, String>`.
- `IndexLspExecutor` implements `LspExecutor`. `definition`, `references`, `hover`, `symbols` and `diagnostics` call `CodeIndexServiceClient` directly with `workspace_root = repo_dir`.
- `select_lsp_executor` (line 388).

**`.../tddy-lsp-executor/src/lsp_tools.rs`**
- `LSP_TOOL_NAMES = ["LspDiagnostics","LspDefinition","LspReferences","LspHover","LspSymbols"]`, gated by `TDDY_LSP_TOOLS`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-toolcall/src/toolcall/lsp.rs`**
- `trait LspExecutor` methods take `(repo_dir: &Path, query: &LspQuery)`. There is no session id.
- The executor is a process-global `OnceLock`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-tool-engine/src/lib.rs`**
- `execute_tool_with_env(worktree_root, tool_name, args, registry, session_id, extra_env)` (line 227).
- The `Lsp*` arm calls `tool_lsp(worktree_root, tool_name, &args)` (line 246). `session_id` is available here but not passed on.
- The `restructure_*` arm calls `restructure_tools::tool_restructure(worktree_root, …)` (line 250).

**`.../tddy-tool-engine/src/restructure_tools.rs`**
- `executor.execute(worktree_root, tool_name, args)`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-toolcall/src/toolcall/restructure.rs:23`**
- `RESTRUCTURE_TOOL_NAMES = restructure_load/check/apply/status/plans/anchors`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-tools/src/restructure_tools.rs`** (MCP side)
- Doc comment: forwarded "by name over the session-tool transport to the host's `ExecuteTool` — exactly the path the `Lsp*` tools take".

**`.../tddy-tools/src/index_client.rs`**
- The CLI `tddy-tools restructure` dials `TDDY_INDEX_SOCKET` directly (a developer opt-in; "set in no session").

**`.../tddy-session-lifecycle/src/connection_service/local_exec_tools.rs`**
- `ExecToolRoute::HostWorktree` → `tool_engine::execute_tool_on_shell(..., &req.session_id)`.
- `ExecToolRoute::Jail(jail)` → `jail.execute_tool(req)`.
- The clone path → `tool_engine::execute_tool(&clone.worktree_path, ...)`.

**`.../tddy-daemon-sandbox/src/conversation_tool.rs:40–50`**
- With a conversation id, tools run in the conversation worktree, which is a different index root.

**`.../tddy-agent-launch/src/jail_env_builders.rs:62–84`** and **`tddy-sandbox-app/src/spawn.rs:687`**
- These set `TDDY_RESTRUCTURE_TOOLS` and `TDDY_LSP_TOOLS` based on the registered executors.

### Tests
**`/Users/mantasi/Code/tddy-coder/packages/tddy-web/cypress/component/CodeIndexStatusStrip.cy.tsx`**
- Fakes: `aHostWarming(progress[], gate, last|"fail")` uses `anInMemoryRpcBackend().implement(CodeNavigationService, { async *watchCodeIndex() {...} })`; `aHostWarmingNothing()` uses an empty generator.
- `mountTheCodePane` mounts `WorktreeCodePane` with both clients on `backend.transport()` through `mountWithRpc`.
- Three tests: progress then ready (`"Indexing — Indexing 38%"`, detail `"Indexing: 120/310 (tddy_core)"`); untracked; failed.

**`.../cypress/support/pages/worktreeCodePanePage.ts:25–35`**
- `indexStatus()`, `indexStatusSummary()` and `indexStatusDetail()`, using `TEST_IDS` from `cypress/support/testIds.ts:211–215`.

**`.../cypress/support/rpc/inMemory.tsx`**
- `mountWithRpc(component, backend)` wraps the component in `RpcTransportProvider` with `httpTransport` and `liveKitFactory` both pointing at `backend.transport()`.

**Other specs**
- `SessionStartAndIndexingProgress.cy.tsx:191–223`: `aHostWarmingTheSessionsIndex` fake ("Fetching" 40%, then ready).
- `WorktreeCodePaneNavigation.cy.tsx:128–134` uses `.onUnary(CodeNavigationService.method.definition|hover|references)` and implements no `watchCodeIndex`.
- `RestructurePlanDialog.cy.tsx` uses `implement(CodeNavigationService, …)`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-daemon/tests/code_index_warmup_acceptance.rs`**
- Uses a real `IndexDaemonRegistry` over a stand-in shell script that binds a fake `code_index` server, whose `Warm` stream the test drives.
- Tests (lines 517–635):
  - `a_session_on_a_rust_worktree_starts_warm_once_its_worktree_exists`
  - `watch_code_index_delivers_phase_percentage_and_ready`
  - `without_an_index_daemon_no_warm_starts`
  - `watching_a_session_nothing_warmed_ends_at_once_with_nothing`
  - `a_warm_failure_is_reported_with_its_reason_and_the_warm_does_not_panic`
  - `another_users_watch_of_the_session_is_refused_and_shows_no_progress`
- Helpers: `a_warm_of`, `until_the_latest_progress_is`, `the_navigation_entry_over`, `watch_code_index_at` (via `entry.service.handle_rpc(name, "WatchCodeIndex", &bytes)`), `everything_delivered_by`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/tests/code_index_service_acceptance.rs`**
- Warm tests: `warms_a_workspace_root_and_reports_it_ready` (line 220), `warming_a_root_forwards_the_servers_phases_and_reports_ready_only_once_it_is_quiescent` (249), `warming_a_root_whose_graph_is_already_loaded_answers_ready_without_waiting_again` (309), `warms_two_workspace_roots_independently` (344).
- Navigation tests: `definition_returns_…` (1443), `references_returns_…` (1474), `hover_returns_…` (1505).
- `a_host_over_fake_language_servers_that_load_a_graph()` (line 57) runs `fake_lsp --loads-crate-graph`. It expects the phases `("loading crate graph",0),(…,25),(…,50),(…,75)`, a `furthest` containing 75%, and only the last message carrying `ready`.

**`/Users/mantasi/Code/tddy-coder/packages/tddy-lsp/tests/bin/fake_lsp.rs`**
- Modes: `--loads-crate-graph` (begin "loading crate graph" → reports → `serverStatus quiescent:true`), `--never-quiescent` ("Building compile-time-deps"), `--goes-busy-after-hovers N`, and cold hovers sending `$/progress`.
- I found no `window/workDoneProgress/create` emission in what I grepped.
- Related tests: `tddy-lsp/tests/notification_observation_test.rs`, `fake_lsp_busy_modes_test.rs`.

### Docs
- `/Users/mantasi/Code/tddy-coder/packages/tddy-web/docs/session-start-and-indexing-progress.md` ("Indexing indicator"): the five states, both components, and the state-to-text table. Testing table: `CodeIndexStatusStrip.cy.tsx` (3), `SessionStartAndIndexingProgress.cy.tsx` (2).
- `/Users/mantasi/Code/tddy-coder/docs/ft/web/session-code-pane.md:95–130`:
  - L97–99: "When a session's worktree is ready … the host loads that worktree's index in the background"
  - L109–118: strip states
  - L121: "`<phase>` is the phase the index is in **now**, and `<n>` the furthest that phase has got"
  - L126: "Ready" means the graph is loaded
  - L129–130: "Not every session warms on start. Sandboxed, tool and split sessions … trigger no warm-up"
- `/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/docs/code-index-service.md`: L61–83 warm-state table; L106–108 "`Warm` streams every phase … `unthrottled()`"; L110–117 "`phase` and `percentage` are the current phase's … `ServerChatter::current()` … `furthest`"; L120ff navigation.
- `/Users/mantasi/Code/tddy-coder/packages/tddy-code-restructuring/docs/readiness-and-gates.md:28–64`: readiness via `ServerChatter.loading()` and `serverStatus`; status kept on `LspClient::server_status`; heartbeat `WAIT_HEARTBEAT` 30s; "A consumer that names what the server is doing **now** reads `ServerChatter::current` … which the index daemon's `Warm` stream does."
- `/Users/mantasi/Code/tddy-coder/packages/tddy-daemon-rpc/docs/architecture.md:31–37,77` and `/Users/mantasi/Code/tddy-coder/packages/tddy-daemon/docs/code-navigation-service.md:66–118`: WatchCodeIndex authorisation, the warm-on-start observer, and the test tables.
- Changesets: `/Users/mantasi/Code/tddy-coder/docs/dev/changesets/2026-10-18-code-pane-index-status-strip.md` (#652) and `/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/docs/changesets/2026-10-18-warm-names-the-current-phase.md` (a real warm took 7m31s and read "Roots Scanned 100%" throughout before the fix).
- Todos (in `/Users/mantasi/Code/tddy-coder/docs/dev/todo/`), summarised under point 6.

---

## 3. Grep and glob log

| Pattern | Scope | Notable hits |
|---|---|---|
| filename `code_navigation*.proto`, `code_index*.proto`, `*_pb*`, `CodeIndexStatusStrip*`, `useCodeIndexProgress*` | repo | the canonical files in `packages/…`; many copies in `.claude/worktrees` and `.worktrees`, ignored |
| `WatchCodeIndex\|watchCodeIndex` | packages rs/ts/tsx/proto/md | `code_navigation.proto:35,55`; `code_navigation.rs:134,232`; `code_index_warmup.rs:8`; `useCodeIndexProgress.ts:40`; cypress specs; `code_index_warmup_acceptance.rs`; `runtime.rs:1146` |
| `CodeIndexStatusStrip\|SessionIndexingIndicator\|CodeNavigationService\|codeNavigationClient` | tddy-web/src | `SessionMainPane.tsx:39,87,196,498,619`; `SessionsDrawerScreen.tsx:507,937`; `WorktreeCodePane.tsx:16,29,138` |
| `IndexWarmupObserver\|SessionWorktreeObserver\|worktree_ready\|SessionIndexProgress\|with_index_progress\|warm_for_session` | packages rs | `runtime.rs:1148,1193,1435`; `session_worktree_observer.rs:17–36`; `svc_start_session_core.rs:335,386,424`; `connection_service.rs:202–214` |
| `resolve_owned_session_dir\|resolve_listed_worktree` | tddy-worktree-service | `service.rs:162,212` |
| `chatter*` (filename) | packages | `tddy-code-restructuring/src/backends/rust/chatter.rs` only |
| `$/progress\|WorkDoneProgress\|workDoneProgress\|cachePriming\|Roots Scanned` | packages rs | tddy-lsp (`client.rs`, `dispatch.rs`, `notifications.rs`, tests, `fake_lsp`); index-daemon `status.rs` and `warm.rs`; restructuring `chatter.rs`, `wait.rs`, `rust.rs` |
| `workDoneProgress\|serverStatusNotification\|with_capabilities` | packages rs | `rust.rs:231–232`; `index-daemon/src/main.rs:214`; tests |
| `cachePriming\|Fetching\|Building CrateGraph\|"Indexing"\|Roots Scanned` | packages | no real rust-analyzer title handling anywhere; only test fixtures (`warm.rs` tests, cypress "Fetching"/"Indexing", `dispatch.rs:144` `"rustAnalyzer/Roots Scanned"`) |
| `TDDY_INDEX_SOCKET\|index_socket\|CodeIndexServiceClient` | packages rs | `tddy-tools/src/index_client.rs`, `restructure_tools.rs`; `tddy-lsp-executor/src/index_backed.rs`, `restructure_via_index.rs`; daemon-rpc |
| `IndexLspExecutor\|IndexChannel for\|select_lsp_executor` | packages rs | `runtime.rs:1250–1262`; `tddy-daemon/src/index_daemon/lsp_channel.rs:13`; `index_backed.rs:388` |
| `execute_tool_with_env\|execute_tool(` | packages rs | `local_exec_tools.rs:164,342`; `conversation_tool.rs:41,50`; `tddy-sandbox-runner/src/runner.rs:2193`; `tddy-coder/.../connection_service_participant.rs:245` |
| `lsp_executor()\|restructure_executor()` | packages rs | `tool-engine/src/lib.rs:275,766`; `jail_env_builders.rs:63,76`; `sandbox-app/src/spawn.rs:687` |
| `TODO(indexing-indicators)` | packages | `svc_start_session_core.rs:59` |
| `worktreeCodeIndexStatus` | tddy-web | `cypress/support/testIds.ts:211–215` |
| `code_navigation` in build configs | packages | `tddy-service/build.rs:210,639`; `tddy-web/buf.gen.yaml` (`out: src/gen`) |

---

## 4. Findings, by your eight points

### 4.1 tddy-web today
- **Hook.** `useCodeIndexProgress(client, sessionToken, sessionId)` in `.../session/useCodeIndexProgress.ts:25` calls `client.watchCodeIndex({sessionToken, sessionId})` on a `Client<typeof CodeNavigationService>`, from `../../gen/code_navigation_pb`.
- **Two consumers share it**, and each opens its own stream:
  - The header indicator, `SessionIndexingIndicator`, mounted at `SessionMainPane.tsx:498–503`.
  - The Code pane strip, `CodeIndexStatusStrip`, at `WorktreeCodePane.tsx:137–143`. It renders only when both `navigationClient` and `sessionId` exist.
  - The pane mounts at `SessionMainPane.tsx:605–624` when `codeOpen && codePaneEnabled && worktreeClient`.
- **Client source.** `SessionsDrawerScreen.tsx:507`: `useDaemonClientFor(CodeNavigationService, selectedOwningHost)`, so it reaches the owning host.
- **Opening the pane is passive.** It only *watches* the session's record. Nothing on the web side asks for a warm. If nothing warmed the session (a restart, a sandboxed start, and so on), the strip reads `untracked`.
- **Not available to the strip today.** It has no `projectId` or `worktreePath` (the pane has both), and it sees only `phase`, `percentage` and `line`; `furthest` is received but not shown.

### 4.2 Protos
- **`code_navigation`** (`/Users/mantasi/Code/tddy-coder/packages/tddy-service/proto/code_navigation.proto`):
  - `WatchCodeIndex(WatchCodeIndexRequest{session_token, session_id}) → stream CodeIndexProgress{line, phase, percentage, furthest, ready, error}`.
  - `Definition`, `References` and `Hover` take `{session_token, project_id, worktree_path, rel_path, position}`.
  - There is no Warm RPC and no worktree-keyed watch.
- **`code_index`** (`/Users/mantasi/Code/tddy-coder/packages/tddy-index-daemon/proto/code_index.proto`):
  - `Warm(WarmRequest{workspace_root}) → stream IndexProgress{line, phase, percentage, furthest, ready}`. It is idempotent.
  - Navigation takes `{workspace_root, file, position}`.
  - The navigation responses carry no index or loading state.
  - `RestructureEvent.indexing` reuses `IndexProgress`.

### 4.3 Daemon side
- **Implementation.** `tddy-daemon-rpc/src/code_navigation.rs` (`CodeNavigationServiceImpl`), registered at `runtime.rs:1426–1436` and sharing `session_index_progress` with the warm observer.
- **Where `SessionIndexProgress` is filled.** Only by `warm_for_session` (`code_index_warmup.rs:88`).
  - It's called from `IndexWarmupObserver::worktree_ready`, installed at `runtime.rs:1188–1199`.
  - That fires from `announce_worktree_ready` (`svc_start_session_core.rs:335,386,424`), i.e. only for workspace starts and non-sandboxed claude-cli and cursor-cli starts.
  - The record is in memory only and lost when the daemon restarts.
  - Nothing deduplicates concurrent warms for one session or root.
  - The first message is synthetic: `phase:"Starting"`.
- **Forwarding.** `authorise_and_connect` runs `resolve_listed_worktree` (token → OS user → project main repo → `git worktree list`), validates the rel path, requires the index daemon, then calls `IndexChannelSource::connect()`. The registry's connect lazily starts the index daemon. The request then goes to `CodeIndexServiceClient::{definition,references,hover}`.
- **Authorisation for the watch.** `WatchCodeIndex` uses `resolve_owned_session_dir` (token → OS user → session directory exists, otherwise `NotFound`). This is a different model from navigation's project-plus-worktree check.
- **Gap for a worktree-keyed warm.** A worktree-keyed warm or watch would need `resolve_listed_worktree` plus a forward to `code_index.Warm`. The todo `2026-10-18-code-index-warm-is-followed-by-session-record.md` proposes exactly this.

### 4.4 tddy-index-daemon and LSP progress
**Yes, it already consumes rust-analyzer's `$/progress` and `experimental/serverStatus`.** The chain:
1. **Advertised capabilities** (`tddy-code-restructuring/src/backends/rust.rs:231–232`): `window.workDoneProgress: true` and `experimental.serverStatusNotification: true`, sent at spawn (`tddy-index-daemon/src/main.rs:207–218`).
2. **Progress-token requests.** `tddy-lsp/src/client/dispatch.rs:62–69` answers rust-analyzer's server→client `window/workDoneProgress/create` with `null`, which is enough for it to start sending progress.
3. **Notification fan-out.** `dispatch.rs:71–74` passes every notification to `NotificationSink::record` (`notifications.rs:101`), which feeds a 256-entry backlog, a 256-capacity broadcast, and a latest-`serverStatus` slot.
   - Subscribers (`LspClient::subscribe_notifications`, `client.rs` around 195) see only notifications that arrive after they subscribe. There is no replay.
   - A subscriber that falls behind gets `Lost(n)`.
4. **The fold.** `ServerChatter` (`chatter.rs`) folds both notifications:
   - It tracks begin, report and end per token and carries the title forward from `begin`.
   - `current()` is the most recently reported phase with the maximum percentage *that token* reached.
   - `furthest()` is the maximum across phases.
   - `how_far()` gives `"{last line}; furthest {phase} {pct}%"`.
   - It also tracks `quiescent`, `loading()` and `degraded()` (health).
5. **Phase titles.** No code special-cases rust-analyzer titles such as "Loading", "Fetching", "Indexing", "cachePriming", "Roots Scanned" or "Building CrateGraph". The title is passed through as `phase` verbatim (or `"working"` if the `begin` wasn't seen).
6. **The warm stream.** `warm.rs::serve_warm`:
   - Calls `client_for` (spawning the server if needed), takes the root's `GraphLoad`, and subscribes.
   - `narrate_until_loaded` sends one `IndexProgress` per distinct line through `phase_of`, since `ServerChatter::unthrottled()` turns off the 2s throttle.
   - It ends with `ready` once `GraphLoad` latches.
7. **The latch.** `GraphLoad` (`graph.rs`) is a per-root latch. It's attached in `WorkspaceIndex::record_use` the first time `client_for` hands out a server (`index.rs:407–426`), and set by `serverStatus{quiescent:true}`. It is never unset. A dead server drops the sender.
8. **How percentage is computed.** `percentage = chatter.current().0`, the highest percentage the current phase's token has reached (0 until that phase reports one), cast to u32. `furthest` is a string, not a number. There is no overall or weighted percentage across phases.
9. **A second `Warm` that joins mid-load starts cold.** It gets a fresh `ServerChatter`, and the broadcast doesn't replay earlier messages, so it has missed the `begin` titles of phases already running. Its reports read as phase `"working"` until a new `begin` arrives, and its `current` and `furthest` start empty. Only `serverStatus` is retained (on `LspClient::server_status()`), and `warm.rs` doesn't read it.
10. **Navigation doesn't wait.** `navigation.rs` calls `client_for` and queries straight away, without checking `GraphLoad`, so it answers empty while loading.
11. **Navigation starts the server but nobody watches.** Any `client_for` call (navigation, Snapshot, LSP tools) spawns rust-analyzer and attaches the latch, so it starts the load. But it publishes no progress anywhere unless a `Warm` stream is open.

### 4.5 MCP code tools
- **Tool sets.**
  - `LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover`, `LspSymbols` (`tddy-lsp-executor/src/lsp_tools.rs:17`), gated by `TDDY_LSP_TOOLS`.
  - `restructure_load`, `restructure_check`, `restructure_apply`, `restructure_status`, `restructure_plans`, `restructure_anchors` (`tddy-toolcall/src/toolcall/restructure.rs:23`), gated by `TDDY_RESTRUCTURE_TOOLS`.
- **Path.** In-jail `tddy-tools --mcp` sends the call over the session-tool transport to the host's `ExecuteTool`:
  - `local_exec_tools.rs` routes to `HostWorktree`, `Jail` or clone.
  - Then `tddy_tool_engine::execute_tool_with_env(worktree_root, …, session_id, …)`.
  - Then `tool_lsp(worktree_root, …)` or `tool_restructure(worktree_root, …)`.
  - Then the process-global executor (`IndexLspExecutor` or `IndexRestructureExecutor`), registered at `runtime.rs:1247–1262`.
  - Then `IndexChannel::connect()` (the daemon's `IndexDaemonRegistry`, `tddy-daemon/src/index_daemon/lsp_channel.rs`).
  - Then `CodeIndexServiceClient` directly.
- **So the calls pass through the daemon process but not through `code_navigation`.** The `LspExecutor` trait (`tddy-toolcall/src/toolcall/lsp.rs:31–52`) and `tool_lsp` take only `(repo_dir, query)`. `session_id` is available in `execute_tool_with_env` but isn't passed down. The natural place to note "interacted" is `execute_tool_with_env` or `local_exec_tools.rs`, where both `req.session_id` and the worktree root are known.
- **Different worktree roots.** Conversation-scoped calls (`conversation_tool.rs`) run in a conversation worktree, so their root differs from the session worktree.
- **Jail route.** Calls go into the jail (`jail.execute_tool(req)`, with `tddy-sandbox-runner/src/runner.rs:2193` running `execute_tool_with_env` inside it). I didn't confirm which executor answers `Lsp*` there.
- **`TDDY_INDEX_SOCKET`** is the CLI's direct dial (`tddy-tools/src/index_client.rs`) and is "set in no session".
- **Today, no MCP interaction triggers a warm or records progress.** It warms rust-analyzer implicitly through `client_for`, but neither `SessionIndexProgress` nor a `Warm` stream sees it.

### 4.6 Session-start warm and the todos
- **Start warm.** `svc_start_session_core.rs` calls `announce_worktree_ready` → `IndexWarmupObserver::worktree_ready` → `warm_for_session`. The `TODO(indexing-indicators)` at line 59 points to the start-phases todo below.
- **The three todos**, in `/Users/mantasi/Code/tddy-coder/docs/dev/todo/`:
  - **`2026-10-03-start-phases-and-code-index-warm-skip-sandboxed-tool-and-split-starts.md`.** No warm (and no `StartPhase`) for sandboxed claude-cli and cursor-cli, tool, split, or pr-stack and grill-me children. "What would close it" asks for `announce_worktree_ready` on each path plus `start_phase_acceptance.rs` cases.
  - **`2026-10-18-code-index-warm-is-followed-by-session-record.md`.** The record is in memory, so after a host restart the strip reads "not warmed" and navigation answers empty. "What would close it": "a pane-side watch that forwards to `code_index.Warm` for its worktree. `Warm` is idempotent … so following it also *starts* the load … Cost: one `code_navigation` RPC with the worktree service's authorisation, a forwarding handler in `tddy-daemon-rpc`, and switching `useCodeIndexProgress`'s source."
  - **`2026-10-18-navigation-answers-empty-while-the-index-loads.md`.** "What would close it": check `GraphLoad` before answering and add an `index_state` field to the three responses in both protos. Waiting on the latch was rejected, because a click mustn't hang for minutes.

### 4.7 Existing tests
- **Web.**
  - `CodeIndexStatusStrip.cy.tsx` has three cases and fakes `CodeNavigationService` with `anInMemoryRpcBackend().implement(CodeNavigationService, { async *watchCodeIndex(){…} })`, gate-driven, mounted through `mountWithRpc` (`cypress/support/rpc/inMemory.tsx`).
  - Page object: `worktreeCodePanePage.indexStatus` / `indexStatusSummary` / `indexStatusDetail`.
  - `SessionStartAndIndexingProgress.cy.tsx` fakes the header stream the same way.
  - `WorktreeCodePaneNavigation.cy.tsx` uses `onUnary` for definition, hover and references and implements no `watchCodeIndex`. Its strip probably ends `untracked`, or errors if the testkit refuses methods it doesn't implement; I didn't check which.
  - The testkit also has `registerServerStreamFallback` (`tddy-connectrpc-testkit`).
- **Daemon.** `tddy-daemon/tests/code_index_warmup_acceptance.rs` has six tests (listed in section 2) and drives `warm_for_session` directly against a fake `Warm`, through the real registry and stand-in script. `code_navigation_acceptance.rs` and `plan_dialog_acceptance.rs` use the same stand-in. There are no `WatchCodeIndex` unit tests inside `tddy-daemon-rpc/src`; its tests there only cover location mapping.
- **Index daemon.** `code_index_service_acceptance.rs` has Warm tests at lines 220, 249, 309 and 344, and navigation tests at 1443, 1474 and 1505, against `fake_lsp --loads-crate-graph`. `warm.rs` has three unit tests; `chatter.rs` has many.
- **tddy-lsp.** `notification_observation_test.rs`, `fake_lsp_busy_modes_test.rs`, and the unit tests in `notifications.rs` and `dispatch.rs` (progress kept and lost).

### 4.8 Doc statements that will need updating
- **`session-start-and-indexing-progress.md`.** `WatchCodeIndex(session)`; the five states; "`untracked` (the stream ended without a message — nothing on the host warms this session, including a warm the host no longer remembers)"; the strip renders only "when it has both a navigation client and a `sessionId`".
- **`session-code-pane.md`.**
  - L97: warm starts "When a session's worktree is ready".
  - L117–118: Nothing warming → "the index loads on the first definition or hover".
  - L121: phase now, `<n>` that phase's furthest.
  - L129–130: sandboxed, tool and split starts don't warm.
- **`code-index-service.md`.** L106–108: "`Warm` streams every phase … `unthrottled()`". L110–117: `phase` and `percentage` are the current phase's (`ServerChatter::current()`), with the cross-phase figure in `furthest`.
- **`readiness-and-gates.md`.** L35–43: readiness via `serverStatus` quiescence, latest status kept on `LspClient::server_status`. L57–59: the index daemon's `Warm` reads `ServerChatter::current`.
- **`tddy-daemon/docs/code-navigation-service.md`.** L66–79: only claude-cli, cursor-cli and workspace starts warm; others load "on its first navigation request". L83–86: watch authorisation through `resolve_owned_session_dir`.

## Exploration 2 — parent: transports, hover rendering, code-issue scan

### Sequence
1. `grep "returns (stream" packages/tddy-service/proto/*.proto` and `grep "stream .*) returns (stream" packages/*/proto/*.proto` — bidi precedent.
2. `grep -rln "bidi" tddy-web/src tddy-livekit*/src` → `useAcpSession.ts`, `authGatedTransport.ts`, `roomTerminalFeed.ts`.
3. Read `tddy-web/src/rpc/daemonTransport.ts:1-60` — browser control plane is `createConnectTransport` (`connect-web`, no full duplex); desktop is `createTauriTransport`.
4. Read `rpc/selectedDaemon.tsx:436` (`useDaemonClientFor` → `useHostClient`), `rpc/connections/registry.tsx:1-60,233` (provider registry: LiveKit + IPC), `rpc/connections/localHost.ts:1-40`.
5. Read `tddy-tauri-web/src/transport.ts:1-30` — "a duplex frame pipe … streaming is `tddy-rpc-web`'s".
6. Read `components/chat/useAcpSession.ts:25-60` — bidi over LiveKit.
7. Read `cypress/component/AgentChatAcpStreamingAcceptance.cy.tsx:55-110` — bidi fake: `async function* session(requests: AsyncIterable<…>)`.
8. Read `components/session/CodeNavigationOverlays.tsx` (83 lines), `useCodeNavigation.ts` (156), `WorktreeCodePane.tsx:60-178`, `CodeBlock.tsx:54,95-140,159-184`.
9. Step 2b scan: `ls packages/<pkg>/docs/code-issues` for tddy-web, tddy-service, tddy-daemon-rpc, tddy-daemon, tddy-index-daemon, tddy-code-restructuring, tddy-lsp, tddy-tool-engine, tddy-lsp-executor, tddy-session-lifecycle, tddy-toolcall; `grep -l "Claimed by:"`; file-name grep across all code-issues.

### Excerpts
- `CodeNavigationOverlays.tsx` `HoverCard`: `<div data-testid="worktree-code-hover" className="pointer-events-auto rounded border … p-2 shadow"><pre className="whitespace-pre-wrap font-mono text-sm">{hover.markdown}</pre>…References / Close`.
- `WorktreeCodePane.tsx`: overlays in `<div className="pointer-events-none absolute inset-x-3 bottom-3 flex flex-col gap-2">`.
- `CodeBlock.tsx:129-136`: `onClick` navigates only with `ctrlKey||metaKey`; `onMouseOver → hoverIdentifier(at, e)`; `HOVER_DWELL_MS = 250`.
- `useCodeNavigation.ts` `navigate`: `found.length === 0 → setNotice({ text: "No definition found" })`.

### Code issues hit
| Issue | Note |
|---|---|
| `packages/tddy-index-daemon/docs/code-issues/oversized-file-index.md` | 522 prod lines, Restructure required, unclaimed |
| `packages/tddy-index-daemon/docs/code-issues/complexity-warm-narrate-until-loaded.md` | nesting 5, unclaimed |
| `packages/tddy-index-daemon/docs/code-issues/poisoned-warm-latch-on-interrupted-index.md` | mechanism 1 does not hold on master; 2 and 3 are restructure CLI concerns |
| `packages/tddy-web/docs/code-issues/oversized-file-session-main-pane.md` | 674/500, regressed by #652 |
| `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` | 816/500, Restructure required |
| `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` | 1,777, Restructure required |
| `packages/tddy-code-restructuring/docs/code-issues/*` (2 claimed: anchors, verify-retarget) | not in path |

# Initial Discovery: dissolve `ConnectionService` — the daemon becomes wiring

**Changeset**: [2026-09-10-unbundle-daemon-becomes-wiring.md](./2026-09-10-unbundle-daemon-becomes-wiring.md)
**Stack**: `#unbundle` node 9 of 9. Explorations 1–6 are the whole-stack discovery; Exploration 7
measures what nodes 1–8 actually leave behind, which is what justified adding this node.

**Date**: 2026-09-09
**Passes**: 6

## Combined Conclusions

### The target state is reachable by removing, not by writing

`tddy-daemon`'s wiring layer **already exists and is already thin — 2,715 prod LoC in six files**
(`main.rs` 168, `startup.rs` 31, `lib.rs` 109, `server.rs` 98, `runtime.rs` 1,041 prod,
`config.rs` 1,268 prod). `runtime.rs` carries the contract to build against — *"assembly: it derives
every service from configuration and returns the handles. Nothing that listens, dials or runs forever
is started there"* — and `tddy_rpc::ServiceEntry { name, service }` is the **only** thing a subsystem
crate must produce. `tddy-tools`' `main.rs` (170) is likewise already the right shape; its `lib.rs`'s
14 public modules are the problem.

### The base branch is a hard requirement, not a preference

All four `tddy-lsp` bridge defects that make `tddy-tools restructure apply` fail on **every**
operation are still present on `master` and on the current branch, verified from code
(`client.rs:395` discards JSON-RPC errors, `lsp_bridge.rs:38` classifies everything as
`MalformedPlan`, `client.rs:20` hard-caps requests at 10s, `client.rs:121` sends
`"capabilities": {}` so rust-analyzer returns no code actions at all and columns are silently wrong
on any non-ASCII line). The fixes exist **only** on
`feature/connection-service-split/lsp-settle-budget`. This independently confirms the chosen base.

That base is also **node 2/2 of an already-open, registered stack** (`master ← lets-list-top-10-source-files`
#467 `← feature/connection-service-split/lsp-settle-budget` #468, draft, slug `#connection-service-split`).
Per the user's decision this stack registers **separately, rooted on #468**, with a new slug — which
creates one standing obligation: **when #468 merges its branch is deleted, and a deleted base branch
CLOSES its dependent PR**, so the new root must be repointed to `master` at that moment.

### What the base branch actually left behind

Its changeset says "plan 1 of 6 applied"; the tree says otherwise. `connection_service.rs` is down to
a **2,416-line facade** over 60 files in `connection_service/` — but one of those,
**`rpc_service.rs`, is 6,278 non-blank lines holding `impl ConnectionServiceTrait for
ConnectionServiceImpl` with all 90 method bodies still inline.** `plan-4-trait-bodies.jsonl` and the
`handlers/*.rs` grouping did not land. That file is the seam this stack cuts, and its own changeset
names the continuation as out of scope for itself: *"splitting `ConnectionService` into several gRPC
services in the `.proto` — a protocol change"*.

### The tooling cannot cross a crate boundary — so the stack extends it first

**Verdict, with five independent proofs**: `MoveSymbol`/`MoveFile` are absent from the Rust backend's
`SUPPORTED` (hard `UnsupportedOp`, never a skip); `RefactorOp::to` is read **nowhere** on the Rust
path; `edit_for` emits exactly one `FileEdit::Change` for the anchor's own file;
`extract_module_to_file` names and places the file itself, always beside the parent; and
`rewrite_import_path` was **deliberately removed** from the vocabulary — *"a vocabulary that
advertises what cannot be performed is worse than a smaller one"* (`plan.rs:40`). Nothing in the crate
reads or writes a `Cargo.toml`, and **no codemod or import rewriter exists anywhere in the repo**.
`rename_symbol` is a trap rather than a workaround: `edits_for` filters rust-analyzer's cross-file
rename edits down to the anchor's own document, so it would break external callers silently.

Per the user's decision the stack therefore **opens with two tooling nodes**: one that stops
`edits_for` discarding other documents' edits (a live defect on its own), and one that adds a real
`move_module_to_crate` operation — `FileEdit::Rename` (which `apply.rs` already `git mv`s), the moved
file's own `use` header, caller re-pointing driven by a real `textDocument/references` result, both
`Cargo.toml`s, and a `reexport` facade in the source crate. Authored by the package but
**engine-informed**, matching the two self-authored transformations that already exist
(`extract_class`, the facade `use` line).

What the tool can already do is not small, and is proven at 23,099-line scale:
`extract_module` + `to_file: true` + `reexport: "glob"` clusters scattered items into one cohesive
file with **zero caller diff**. And `refuse_stranded` is a **free cross-crate blast-radius report** —
`reach_of` counts any reference whose URI differs from the anchor's, other crates included, so running
`check --deep` with `reexport` omitted makes the tool name every externally-reached item and every
referring file.

### Coupling is far weaker than the line counts suggest

Of 106 daemon modules, only **eight files outside `connection_service` hold a real code reference to
it**, and four of those are a single symbol each; every other apparent reference is a doc comment.
**Five shared symbols and nine cycles are the whole gate** to a crate split:

| Must leave `connection_service` first | Where | Reached by |
|---|---|---|
| `AgentActivityHub` | `:1212` | SANDBOX (×5), SESSIONS (`session_agent_inference.rs:36`) |
| `now_unix_ms()` | `:1195` | SANDBOX (×2), TELEGRAM (`telegram_session_subscriber.rs:122`) |
| `HOST_DOCUMENT_FRAME_BYTES` | `:18060` | CONTEXT (`context_files.rs:41`) |
| `SessionUserResolver` / `SessionsBaseResolver` | `:236`/`:239` | AUTH (`auth.rs:25`) + 5 subsystems |
| the 9-symbol spawn preamble | 9 sites | SESSIONS (`cursor_cli_spawn.rs`, 7 sites) — the worst edge |

Cycles to cut: `config.rs:85 → session_room` (else the wiring layer depends on LIVEKIT),
`auth.rs:25 → connection_service`, `host_tooling ⇄ ssh_agent` (splits HOSTS from AUTH),
`livekit_peer_discovery ⇄ multi_host`, `common_room_supervisor → daemon_config_service →
livekit_peer_discovery`, `context_files ⇄ worktree_files`, `session_agent_status ⇄
session_agent_inference`, plus two benign internal ones. **That lifting-and-cutting is the natural
root node.**

Working in the stack's favour: **30 `pub trait` ports already exist** and every one of
`ConnectionServiceImpl`'s 21 `with_*` builders injects behind a trait; the extraction pattern is
**already established three times** (`tddy-task`, `tddy-pty`, `tddy-tool-engine`) with 5–7 line
`pub use` shims; and only **three crates link `tddy-daemon` as a library**, `tddy-desktop` consuming
exactly the wiring layer (`{config, runtime, supervisor_client, spawn_worker, cli_session_manager}`).

### Five subsystems need no protocol change at all

`ConnectionService` is **90 methods in 20 documented families** (69 unary, 21 server-streaming, 1
bidi). But models/ACP, screen sharing + VNC, auth + token, telegram
(`observer.proto`/`presenter_intent.proto` — **no `ConnectionService` methods at all**) and the
`tasks`/`actions`/`bsp`/`remote_git`/`session_admission`/`vm` services **already have their own
protos**. Those moves are pure relocation, and they are the right first nodes: `model_registry/`
(3,035 LoC, **0 inline tests**, 0 outbound edges), TELEGRAM (8,036 LoC, `teloxide` leaves with it),
SCREENSHARE (2,299, zero `connection_service` edge), SANDBOX (2,171 prod vs 5,494 integration-test
LoC — the best test locality in the crate, and the move **reverses `tddy-sandbox-app`'s dependency**).

The **terminal family is already extracted and serving nothing**: `terminal_session.proto` duplicates
family K exactly, is referenced nowhere outside its own package, and its consumers hand-convert
between the two message sets (`connection_service.rs:439-457`). That is both a free node and the
cautionary precedent — **extracting a proto without moving the served coordinate leaves dead code
plus a converter.**

### What the proto split really costs

~25 messages are reached from more than one family and need a shared `types.proto`; **four method
pairs share a request type outright** (`StartSession`/`StreamStartSession`,
`ReadWorktreeFile`/`StreamReadWorktreeFile`, `ReadHostDocument`/`StreamReadHostDocument`,
`ExecuteTool`/`StreamExecuteTool`) and cannot be split apart. Three `.connection.*` **extern paths in
the sandbox tonic pass** break unless re-pointed. Adding a service is a one-line `ServiceEntry` on
every transport **except** the UDS/tonic one, where it costs a hand-written adapter per service
because `tddy-codegen`'s `generate_tonic_adapter` is a documented stub.

On the client side the news is good: `useHttpClient(service)` and
`clientFor<S extends DescService>(service)` are **already service-generic**, so the web transport
layer needs no redesign — the migration is import paths (116 in 112 `src/` files, 130 in 126
`cypress/` files), 86 call sites, six hard-coded `ConnectionService` bindings in `src/rpc/`, and a
736-line Cypress fake plus ~10 siblings. And **splitting the proto costs nothing in bun dependency
resolution**: the generator already runs over the whole proto *directory*, so N protos produce N
`*_pb.ts` files with no config change, and `resolve-local-lock.ts` passes `workspace:` specifiers
through untouched.

**`tddy-coder` is a second, partial server for families K/L/M/N and must move in lockstep** — the repo
has already hit and documented the failure where "the same session would have opened tail-first when
reached over HTTP and head-first when reached over LiveKit".

### Free scope discovered along the way

**1,008 LoC of `vnc_service`/`vnc_vault` (plus tests) is unreachable** — `runtime.rs` never registers
`vnc.VncService`. `tool_catalog_sync.rs` is a **test file living in `src/`**. `codex_oauth_relay.rs`
is reached only from `tddy-integration-tests`. Three hand-maintained duplications exist **because**
their originals are locked in binary crates: the `projects.yaml` schema (`tddy-livekit`), and the tool
catalog (`tddy-sandbox` and `tddy-tools/server.rs`, "verbatim", guarded by matched tests). In
`tddy-tools`, `build_cli::plugin_registry` duplicates `tddy_bsp::plugins::plugin_registry` outright.
Moving these into libraries **deletes copies rather than adding files**.

### Prerequisites and hazards

- ⛔ **`run_server`'s 12 positional arguments** (`docs/dev/todo/2026-09-06-…`) — every node changes
  that list, and the entry already scoped the fix to its own PR because it moves the out-of-gate
  `tddy-desktop` caller.
- ⛔ **Generated TypeScript is already stale and nothing detects it** — no CI step runs `buf` or diffs
  `*_pb.ts`; regenerating `tddy-rust-typescript-tests/gen` produces a 5,182-line diff plus 12
  uncommitted files. With every node regenerating TS, drift becomes unattributable unless this is
  fixed first.
- ⚠ **`tddy-desktop` is outside the CI gate** and embeds the daemon, so a green stack can still break
  it. It is the one consumer a node can break invisibly.
- ⚠ **`tddy-testing-commons` ↔ `tddy-tools` is an existing prod/dev dependency cycle** that any change
  to `tddy-tools`' public surface touches first.
- ⚠ **Three import cycles with `tddy-tools/src/server.rs`** (`action_tools`, `lsp_tools`,
  `session_agents/{seed,stream}`) over eight small shared items — one shared primitives module breaks
  all three, and it gates every `server.rs` seam move.
- CI gates `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo nextest run --workspace`, so **a new crate is covered the moment it is a workspace member**.
- Recorded pre-existing failure to carry forward, not re-diagnose:
  `cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`
  — `ConnectionServiceImpl::self_arc called before set_self_handle`, same root cause as the five known
  `sandbox_behavior_acceptance` failures on `master`.

### The shape every node inherits

Because the tooling cannot cross a crate boundary by itself, each subsystem node is three phases with
a clean tool/hand boundary: **(A)** tool-only clustering inside the source crate — `extract_method` on
oversized members, hand-inserted `impl` boundaries where a block must be divided (the only
hand-written lines, verified as a zero-moved-line diff), then `extract_module` + `to_file` + `glob`
per seam in definitions-first order, one plan at a time with `check --deep` → `--dry-run` → apply →
`verify --against HEAD` → prune the parent's now-stale imports → commit → `rm -rf .restructure/`;
**(B)** the crossing, mechanical once node 2 lands; **(C)** the obligations the tool creates —
relocated `impl` members stay `pub(crate)` by design, stale imports must be pruned per plan, and the
doc triage grep must be re-run against what actually landed.

## Exploration 1: package-level size and base-branch state — 2026-09-09

**Agent**: parent Bash (`find` / `wc` / `git show` / `gh`)
**Scope**: whole workspace LoC by package; `tddy-daemon` and `tddy-tools` file inventory; state of the
base branch and its in-flight changeset.

### Sequence

1. `ls packages/` — enumerate the 68 workspace packages.
2. Python line-count sweep over `packages/**` (non-blank lines, `.rs/.ts/.tsx/.js/.css/.html/.py/.sh/.proto/.nix`,
   excluding `node_modules`, `target`, `dist`, `storybook-static`, `generated`) — size the two target packages
   relative to everything else.
3. `find packages/tddy-daemon/src -maxdepth 1 -name '*.rs'` + per-file line counts — inventory of the
   daemon's flat module list.
4. Same for `packages/tddy-tools/src`.
5. `git log --oneline master..origin/feature/connection-service-split/lsp-settle-budget` — what the base
   branch carries over master.
6. `git show <base>:packages/tddy-daemon/src/connection_service.rs | grep -c .` vs the same on
   `origin/master` — how far the in-flight split has got.
7. `git ls-tree <base> docs/dev/1-WIP/` — which changesets the base inherits.
8. `gh pr list --state open` — the base's own PR and its base.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| find | `-maxdepth 1 -name '*.rs'` | `packages/tddy-daemon/src` | 108 flat modules, one subdir (`model_registry/`, 3,035 lines) |
| find | `-name '*.rs'` | `packages/tddy-tools/src` | 28 files; `server.rs` 3,842, `session_tool_client.rs` 991, `cli.rs` 927, `pty_relay.rs` 843 |
| git show | `<base>:…/connection_service.rs` | — | 2,416 non-blank lines (23,099 on `origin/master`) |
| git ls-tree | `docs/dev/1-WIP/` | `<base>` | `2026-08-31-split-sandbox-orchestration.md`, `2026-08-31-split-sandbox-resume.md`, `2026-09-09-connection-service-split{,-initial-discovery}.md`, `connection-service-split/` |

### Inspected files

#### workspace LoC by package (top of the sweep)

**Why**: establish that `tddy-daemon` and `tddy-tools` are the outliers this stack targets, and which
packages already exist as plausible destinations.

```
package                            src LoC  files  test LoC  files
tddy-daemon                          82546    118     58227    168
tddy-web                             63013    387     72626    468
tddy-core                            28872     99      6293     39
tddy-workflow-recipes                20099     90      8696     55
tddy-service                         11564     44        15      1
tddy-tools                           11324     29     10504     50
tddy-coder                           10054     21      5517     24
tddy-supervisor                       8445     21      1904      8
tddy-tui                              8086     16       358      5
tddy-code-restructuring               7824     13         0      0
tddy-vm                               6791     13      6059     27
tddy-sandbox-app                      5577     10       834      1
tddy-sandbox-runner                   4165      5       967      6
tddy-discovery                        3488      9      2652     13
tddy-livekit                          3296     20      2408     11
TOTAL                               318772           217855
```

#### `packages/tddy-daemon/src` — the flat module list (largest first)

**Why**: the candidate seams for a package-level split are visible in the names; the daemon holds whole
subsystems (telegram, hosts, sandbox, livekit, worktrees, sessions, auth) as sibling flat modules.

```
23099  connection_service.rs      (2,416 on the base branch)
 4158  telegram_session_control.rs
 2815  session_room.rs
 2296  spawner.rs
 2215  livekit_peer_discovery.rs
 2204  config.rs
 1953  screen_sharing_service.rs
 1846  telegram_notifier.rs
 1740  session_list_enrichment.rs
 1538  cli_session_manager.rs
 1437  connection_tonic_adapter.rs
 1173  session_agent_clone.rs
 1142  split_session.rs
 1089  runtime.rs
 1058  auth.rs
 1017  sandbox_session.rs
  989  host_tooling.rs
  974  host_registry.rs
  967  worktrees.rs
  880  host_private_key.rs
  787  remote_git_service.rs
  768  common_room_supervisor.rs
  737  telegram_bot.rs
  723  ssh_agent.rs
  695  livekit_rooms_stream.rs
  ... 83 more modules, tapering to 5 lines
  168  main.rs
  109  lib.rs
   98  server.rs
```

#### `packages/tddy-tools/src`

**Why**: same question for the tools binary.

```
3842  server.rs
 991  session_tool_client.rs
 927  cli.rs
 843  pty_relay.rs
 685  session_agents/registry.rs
 461  session_agents/stream.rs
 451  github_pr.rs
 343  action_tools.rs
 302  remote_cli.rs
 292  schema.rs
 275  session_hook.rs
 271  session_agents/conversation.rs
 194  build_cli.rs
 170  main.rs
 ... 14 more
```

#### `docs/dev/1-WIP/2026-09-09-connection-service-split.md` (on the base branch)

**Why**: the base branch is mid-refactor; this stack must not re-plan or contradict it.

Established by that document:

- The split is **intra-package**: `connection_service.rs` → a facade over ~75 modules under
  `connection_service/`, `reexport: "glob"` on every seam, **no caller outside the package in the diff**.
- Seven numbered plans, driven by `tddy-tools restructure` (`extract_module_to_file`, `extract_module`,
  `extract_method`); only step 6 is hand-written (`impl` block boundaries).
- Four `tddy-tools restructure` **transport defects** (bridge path) had to be fixed before any operation
  worked: JSON-RPC `error` discarded, every failure classified `MalformedPlan`, a hardcoded 10s request
  cap `--indexing-budget` could not reach, and `"capabilities": {}` on the bridge client (which made
  rust-analyzer return no code actions at all, and left utf-16 vs utf-8 column mismatch unchecked).
- **`impl ConnectionServiceTrait for ConnectionServiceImpl` floors at ~680 lines** — 111 members, 385
  lines of signatures. The document names going below that as requiring
  "splitting `ConnectionService` into several gRPC services in the `.proto` — a protocol change, out of
  scope for a behaviour-preserving restructure". **That protocol change is in scope for this stack.**
- Recorded baseline: `./test -p tddy-daemon` **1027 passed / 1 failed** (25 suites); the one failure is
  `cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available`,
  `ConnectionServiceImpl::self_arc called before set_self_handle` — same root cause as the five known
  `sandbox_behavior_acceptance` failures on master, pre-existing, not a regression.

### Findings

1. **The base is an unfinished draft PR.** #468's changeset has `Baseline` and `Code Quality`
   unchecked, and its `Applied so far` section documents only plan 1 while the tree shows plans 2–7
   landed. Anything this stack bases on it inherits both the tree and the unfinished document.
2. **The intra-package split is done or nearly done; the package-level split is untouched.** Every one of
   the 108 daemon modules is still in `tddy-daemon`.
3. **The proto is the hard boundary.** #468 explicitly deferred splitting `ConnectionService` into
   several gRPC services as out of scope. The user's request makes it in scope, and it is what makes
   this stack breaking: RPC coordinates move, so `packages/tddy-web` (and any other client) must be
   migrated in the same node that moves them.
4. **`gh stack` linearity constraint applies to an already-registered line.** #467 → #468 is a
   registered two-node stack titled `(#connection-service-split K/2)`. Extending it renumbers both;
   registering a second stack on top of #468 leaves the two lines unrelated on GitHub.

## Exploration 2: base-branch split state, crate-level consumers, backlog, CI gate — 2026-09-09

**Agent**: parent Bash (`git ls-tree` / `git show` / `grep` / `cat`)
**Scope**: what the base branch's split actually produced (as opposed to what its changeset says);
which crates depend on `tddy-daemon` / `tddy-tools` as libraries; the `docs/dev/todo/` cross-check
(Step 2b); the CI gate; the workspace manifests and the local bun resolution scripts.

### Sequence

1. `git ls-tree -r --long <base> packages/tddy-daemon/src/connection_service/` — the real post-split
   layout, with byte sizes, because the changeset only documents plan 1.
2. `git show <base>:…/connection_service/rpc_service.rs | grep -c .` and `grep -c 'async fn'` — size
   the one file the split did not finish.
3. `grep -n '^impl'` in that file — confirm which impl block it holds.
4. `grep -rln 'tddy-daemon' packages/*/Cargo.toml` and the same for `tddy-tools` — the real
   crate-level dependents.
5. `grep -rn "tddy_daemon::" --include="*.rs" packages | grep -v '^packages/tddy-daemon/'` — every
   external reference, then per-file inspection to separate code from doc comments.
6. `ls docs/dev/todo/` + `grep -ril` for `connection_service`, `ConnectionService`, `.proto`,
   `own crate`, `move.*package` — Step 2b candidate set; then read the bodies.
7. `cat Cargo.toml` (workspace members) and `cat package.json` (bun workspaces + scripts).
8. `sed -n '1,90p' docs/dev/guides/ci.md` — what gates a PR.
9. `ls packages/tddy-daemon/docs/`, `packages/tddy-tools/docs/`, `docs/ft/daemon/` — the documents a
   subsystem move has to carry.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| git ls-tree | `connection_service/` | `<base>` | 60 files; `rpc_service.rs` **315,469 bytes**, next largest 52,328 |
| git show + grep -c | `.` | `<base>:…/rpc_service.rs` | **6,278 non-blank lines** |
| git show + grep -c | `async fn` | `<base>:…/rpc_service.rs` | **90** |
| git show + grep -n | `^impl` | `<base>:…/rpc_service.rs` | `400:impl ConnectionServiceTrait for ConnectionServiceImpl {` … `6940:}` |
| grep -rln | `tddy-daemon` | `packages/*/Cargo.toml` | tddy-e2e, tddy-integration-tests, tddy-pty, tddy-sandbox-app, tddy-remote-git-repo, tddy-supervisor, tddy-terminal-rpc, tddy-desktop/src-tauri |
| grep -rln | `tddy-tools` | `packages/*/Cargo.toml` | tddy-sandbox-darwin, tddy-testing-commons, tddy-daemon, tddy-sandbox-app, tddy-terminal-rpc |
| grep -rn | `tddy_daemon::` | `packages/**.rs` minus the daemon | **36 references in 20 files** |
| grep -ril | `connection_service`/`ConnectionService`/`.proto` | `docs/dev/todo/` | 26 of 111 entries |

### Inspected files

#### `<base>:packages/tddy-daemon/src/connection_service/` — the split as it really landed

**Why**: the in-flight changeset says "plan 1 of 6 applied", the commit log says every impl block moved.
Neither states where the trait impl ended up.

```
315469  connection_service/rpc_service.rs          ← 6,278 non-blank lines, 90 async fns
 52328  connection_service/host_add_key_handler_tests.rs
 51546  connection_service/agent_activity_unit_tests.rs
 42051  connection_service/svc_start_session_core.rs
 30848  connection_service/svc_start_sandboxed_claude_cli_session.rs
 ... 55 more files
```

Totals: `connection_service/` = **21,381** non-blank lines across 60 files, plus a **2,416**-line
`connection_service.rs` facade — i.e. the original ~23,099 lines redistributed, with **one file still
holding a quarter of it**.

```rust
400:impl ConnectionServiceTrait for ConnectionServiceImpl {
// ... 90 × async fn, bodies still inline
6940:}
```

**This is the seam this stack inherits.** #468's changeset planned `plan-4-trait-bodies.jsonl` to
`extract_method` each of the 90 bodies down to a delegating line, floors the block at ~680 and groups
the bodies into `handlers/*.rs` by RPC family. That did not land: the bodies are still inline in one
6,278-line file. The changeset also names the reason it could go no further —
*"splitting `ConnectionService` into several gRPC services in the `.proto` — a protocol change, out of
scope for a behaviour-preserving restructure"* — which is exactly what this stack does.

#### crate-level dependents of `tddy-daemon`

**Why**: "leave only high-level wiring" is only meaningful if you know who consumes the crate as a
library today.

Real `Cargo.toml` dependents (8): `tddy-e2e`, `tddy-integration-tests`, `tddy-pty`,
`tddy-sandbox-app`, `tddy-remote-git-repo`, `tddy-supervisor`, `tddy-terminal-rpc`,
`tddy-desktop/src-tauri`.

36 `tddy_daemon::` references outside the crate. The symbols reached are narrow and mostly
wiring-shaped:

```
tddy_daemon::config::{DaemonConfig, DaemonConfig::load, LiveKitConfig::common_room_enabled,
                      RelayConfig, TelegramConfig}
tddy_daemon::connection_service::{ConnectionServiceImpl, AgentActivityHub,
                                  EXEC_TOOL_FRAME_BYTES, HOST_DOCUMENT_FRAME_BYTES,
                                  validate_repoint_target}
tddy_daemon::auth::{build_auth_entries, LiveKitTokenServiceImpl::new}
tddy_daemon::action_service::ActionServiceImpl
tddy_daemon::claude_cli_session::{ClaudeCliSessionManager, CliSessionManager, PtyHandle, strip_resize}
tddy_daemon::cursor_cli_spawn::spawn_cursor_cli_session_inner
tddy_daemon::livekit_peer_discovery::{CommonRoomPeerRegistry::new, daemon_rpc_identity}
tddy_daemon::{active_elicitation, base_sync_cache, branch_owner::find_session_owning_branch,
              context_files::read_context_file_bytes, context_sync::LocalWorktreeSource,
              codex_oauth_relay, daemon_config_service, github_pr_credentials, github_token_store,
              host_documents::{MAX_HOST_DOCUMENT_BYTES, validate_staged_attachment_relative_path},
              host_private_key, host_prompt_stream, host_prompts, host_tooling,
              agent_list_mapping::agent_allowlist_rows, elicitation}
```

`packages/tddy-desktop/src-tauri/src/lib.rs` alone holds 8 of the 36 — it *is* a second wiring
front-end for the same daemon, and `docs/dev/todo/2026-09-05-…-tauri-desktop-single-process-daemon.md`
records that the desktop app "is `tddy-daemon` in one process".

#### four `tddy_daemon::` references that are duplication markers, not dependencies

**Why**: checked whether crates outside the 8 dependents secretly reach into the daemon. They do not —
every hit is a doc comment. But each one records a **copy** that this stack can collapse:

```rust
// packages/tddy-livekit/src/projects_registry.rs:1
//! Reads `projects.yaml` using the same on-disk schema as [`tddy_daemon::project_storage`].
//   :10  /// One project row — mirrors `tddy_daemon::project_storage::ProjectData` for YAML compatibility.

// packages/tddy-sandbox/src/lib.rs:49
/// Must stay in sync with `tddy_daemon::tool_catalog::tool_catalog`.

// packages/tddy-tools/src/server.rs:1577
/// `tddy_daemon::tool_catalog::tool_catalog()` verbatim (adapted from `ToolDef` to …

// packages/tddy-service/src/token_service.rs:31
/// `tddy_daemon::livekit_peer_discovery::daemon_rpc_identity` composes its identities from this
// packages/tddy-core/src/backend/mod.rs:734
/// (`tddy_daemon::connection_service`) writes the six Claude Code hooks that report the session's
```

Three hand-maintained copies (`projects.yaml` schema, the tool catalog ×2) exist **because** the
originals are locked inside a binary crate. Moving them into libraries deletes the copies — value
beyond line counts.

#### `docs/dev/todo/` — Step 2b verdicts

**Why**: 26 of the 111 entries name `connection_service`, the proto, or the packages in play.

| Entry | Verdict | Why |
|---|---|---|
| `2026-08-29-connection-service-rs-is-19-600-lines.md` | ℹ **Answered / superseded** | Names the cohesive groups a split would follow — *session lifecycle, agent roster + conversation, streaming replay handlers, PR-stack and changeset mutations, peer routing* — and the cost: "`ConnectionServiceImpl`'s ~60 private fields would have to become `pub(crate)` or move behind accessors". That field-visibility cost is now **the** central constraint on a cross-crate move, not just a module move. |
| `2026-09-06-connection-service-rs-is-22800-lines.md` | ℹ **Answered / superseded** | Adds the host-facing group (host registry, tooling probe, prompt handlers) as "a further cohesive seam". |
| `2026-09-06-server-rs-run-server-takes-12-positional-arguments.md` | ⛔ **Blocking, small** | `run_server` already carries `#[allow(clippy::too_many_arguments)]`; every subsystem this stack removes changes its argument list. Fixing it to an options struct is a **prerequisite**, and the entry itself says it "moves `main.rs` and the desktop caller, and `tddy-desktop` is outside the CI gate — so it wants its own PR". |
| `2026-09-05-…-tauri-desktop-single-process-daemon.md` | ⚠ **During** | `tddy-desktop` is a second embedder of the daemon and is **outside the CI gate**. Every node that changes the daemon's library surface can break it with green CI. Also records that the Telegram "started/stopped" message is stuck inside `server::run_server` and "sharing it needs that message moved out of the HTTP server" — which this stack does. |
| `2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md` | ⚠ **During** | A duplicated helper inside the file being carved up; each node should carry it to one home rather than copy it per crate. |
| `2026-08-23-the-action-tools-are-advertised-where-nothing-implements-them.md` | ⚠ **During** | Concerns `tddy-tools` action tools — in the path of the tools thinning. |
| `2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md`, `2026-08-15-livekit-connect-streamer-duplication-still-outstanding.md` | ⚠ **During** | Both sit in the LiveKit/streaming subsystem a node will move; a move must not re-introduce or deepen them. |
| `2026-09-06-…-gen-daemon-config-pb-ts-was-regenerated-without.md`, `2026-08-14-tddy-rust-typescript-tests-gen-is-badly-stale-and-nothing-detects-it.md` | ⛔ **Blocking, shared** | Generated TS is already stale and **nothing detects it**. This stack regenerates TS on every node that moves an RPC; a stale-gen check is a prerequisite, or drift becomes indistinguishable from this stack's own breakage. |
| ~85 others | — **Unrelated** | Same packages, different concerns. |

#### CI gate — `docs/dev/guides/ci.md`

**Why**: a 10–16 node stack lives or dies on what the gate proves per node.

```
Rust lint   cargo fmt --all --check, cargo clippy --workspace --all-targets -- -D warnings   ~8 min
Rust build  cargo build --workspace --bins --examples                                        ~10 min
Rust tests  cargo nextest run --workspace --profile ci                                       ~15 min
Web tests   bun install --frozen-lockfile, bun run build, tddy-web unit, tddy-web +
            tddy-livekit-web Cypress component                                               ~10 min
```

Clippy and nextest are **`--workspace`**, so a new crate is gated the moment it is a workspace member —
no per-crate CI wiring needed. Deliberately **excluded**: `tddy-desktop` (needs Electron/Tauri),
`tddy-rust-typescript-tests`, Cypress **e2e**, VM-backed tests, cgroups sandbox tests.
**`tddy-desktop` being outside the gate is the main hidden risk in this stack.**

#### workspace manifests

`Cargo.toml`: 62 members, flat `packages/<name>` list plus `packages/tddy-desktop/src-tauri`. A new
crate is one line here. `[workspace.dependencies]` holds only `rstest`, `pretty_assertions`, `sysinfo`
— so **every crate pins its own versions**; a new crate copies dependency lines rather than inheriting.

`package.json`: bun workspaces are `tddy-web`, `tddy-livekit-web`, `tddy-rpc-web`, `tddy-tauri-web`,
`tddy-desktop`, `tddy-rust-typescript-tests`, `tddy-connectrpc-testkit`, and the local-resolution
scripts the user's brief refers to:

```json
"resolve-local-lock": "bun run scripts/resolve-local-lock.ts",
"local-install": "scripts/local-bun-install.sh",
"local-registry-install": "bun run resolve-local-lock && bun run local-install"
```

#### documents a subsystem move has to carry

`packages/tddy-daemon/docs/` maps almost one-to-one onto the subsystem seams — `host-registry.md`,
`host-tooling-probe.md`, `host-add-key.md`, `model-registry.md`, `remote-git-service.md`,
`session-agent-roster.md`, `session-notifications.md`, `session-room.md`, `telegram-notifier.md`,
`telegram-github-link.md`, `worktrees.md`, `codex-oauth-relay.md`, `oauth-loopback-tunnel.md`,
`agent-session-status.md`, and `connection-service.md` (**931 lines**, an endpoint-by-endpoint
reference for the service being carved up). `packages/tddy-tools/docs/` holds only `json-schema.md`.
`docs/ft/daemon/` holds 26 product docs.

Per CLAUDE.md, `packages/*/docs/` is never edited directly — each node's doc move goes through its own
changeset and `/wrap-context-docs`.

### Findings

1. **The base branch left one 6,278-line file holding 90 trait-method bodies.** The intra-package split
   redistributed 23,099 lines into 60 files but did not finish: `connection_service/rpc_service.rs`
   still carries `impl ConnectionServiceTrait for ConnectionServiceImpl` with every body inline. That
   file is the seam this stack cuts, and cutting it **by RPC family into separate gRPC services** is
   the continuation #468's own changeset named as out of scope for itself.
2. **The daemon's library surface is already narrow (36 references, 8 dependent crates).** The
   subsystems are internally coupled but externally almost unreferenced, so a cross-crate move is
   mostly an internal-visibility problem, not a caller-churn problem.
3. **`ConnectionServiceImpl`'s ~60 private fields are the central constraint**, exactly as the
   2026-08-29 backlog entry predicted — a cross-crate move needs each subsystem's state behind an
   owned struct or a trait, not `pub(crate)` widening, because `pub(crate)` does not cross a crate
   boundary.
4. **Three hand-maintained duplications exist because the originals are stuck in a binary crate**
   (`projects.yaml` schema in `tddy-livekit`, the tool catalog in `tddy-sandbox` and `tddy-tools`).
   Moving those into libraries deletes copies rather than adding files.
5. **`run_server`'s 12 positional arguments are a hard prerequisite**, not a nicety: every node changes
   that signature, and the backlog entry already scoped the fix to its own PR because it moves the
   out-of-gate `tddy-desktop` caller.
6. **Stale generated TypeScript is undetected today.** With every node regenerating TS, that must be
   fixed first or drift is unattributable.
7. **CI gates `--workspace`, so new crates are covered automatically** — but `tddy-desktop` is outside
   the gate and embeds the daemon, so a green stack can still break it.

## Exploration 3: `tddy-tools` inventory and destination-package survey — 2026-09-09

**Agent**: Explore subagent
**Scope**: every module in `packages/tddy-tools/src`, its LoC, purpose, intra-crate coupling and
natural destination; the 50 test files and what each covers; a survey of 28 candidate destination
packages; which destinations each of `tddy-tools` / `tddy-daemon` already depends on.

### Sequence

1. `ls packages/`; `find packages/tddy-tools/src -type f`; `find …/tests -type f` — file roster.
2. `git status --short`; `head -80 Cargo.toml` — confirm read-only; workspace members.
3. `grep -c .` per `src/*.rs` and `tests/*.rs` — the LoC metric.
4. `cat packages/tddy-tools/Cargo.toml` — dependency inventory.
5. Read `src/main.rs`, `src/lib.rs`, `src/cli.rs` in full — the CLI surface.
6. `grep -n '^fn|^struct|#[tool|tool_router'` over `src/server.rs` — map the 3,842-line file before reading.
7. Read `server.rs` in seven regions (1-50, 283-683, 694-993, 1449-1698, 1699-2179, 2179-2378, 2870-3069).
8. `grep -rn 'crate::' src/` — intra-crate adjacency.
9. `grep -o 'tddy_[a-z_]*'` per module — workspace-crate deps per module.
10. Read the 7 small modules whole; heads + public API of the mid-size ones.
11. Per-package loop over 28 destinations: description, src LoC, README, `[dependencies]`.
12. `awk '/^\[/{sec=$0} /tddy-tools/'` over the 5 reverse-dependents — separate dep from dev-dep.
13. `grep -rn 'tddy_tools::'` workspace-wide minus tddy-tools — the real external surface.
14. Per-test-file LoC + first doc line + `tddy_tools::` imports — test→module mapping.
15. `grep -rho 'TDDY_[A-Z_0-9]*'` — env-var coupling.
16. `grep -n '^#\[cfg(test)\]'` per module — prod vs inline-test LoC split.

### Grep / glob

| Tool | Pattern | Path scope | Notable hits |
|---|---|---|---|
| grep -rn | `crate::[a-z_]*` | `tddy-tools/src/**` | 17 distinct intra-crate edges; **three import cycles with `server.rs`** |
| grep -rn | `tddy_tools::` | workspace minus tddy-tools | Only **4** API surfaces used externally; **no production code outside tddy-tools calls `tddy_tools::` at all** |
| grep | `tddy-tools` | all `Cargo.toml` | 5 reverse-dependents; **4 of 5 are `[dev-dependencies]`** |
| grep -rho | `TDDY_[A-Z_0-9]*` | `src/**` | 25 distinct env vars; `TDDY_SOCKET` ×43, `TDDY_REPO_DIR` ×24, `TDDY_SESSION_DIR` ×12 |
| grep -n | `OnceLock\|Mutex::new` | server, session_tool_client, session_agents | **3 process-global singletons** |
| grep -n | `^#\[cfg(test)\]` | `src/**` | 10 of 28 modules; `server.rs` tests start at L3063 (1,089 of 4,152 raw lines) |

### Inspected files

#### `src/main.rs` (170) — already the wiring the brief asks for

`main.rs` declares 7 binary-only modules (`analyze_cli`, `build_cli`, `cli`, `pty_relay`,
`remote_cli`, `restructure_cli`, `session_hook`); `lib.rs` (21 lines) exports 14 public modules.
`--mcp` is a top-level bool, not a subcommand:

```rust
struct Args {
    /// Run as MCP server (stdio transport). Used by Claude Code --permission-prompt-tool.
    #[arg(long)]
    mcp: bool,
    #[command(subcommand)]
    subcommand: Option<Subcommand>,
}
```

**20 subcommands and their dispatch targets** (`main.rs:34-100` decls, `139-164` dispatch): `submit`,
`ask`, `spawn-conversation`, `list-tools`, `call-tool`, `transition`, `get-schema`, `list-schemas`,
`set-session-context`, `persist-changeset-workflow`, `list-actions`, `invoke-action`, `build-list`,
`build`, `pty-relay`, `remote` (4 sub), `session-hook`, `list-models`, `analyze` (3 sub),
`restructure` (5 sub). Plus `init_logging()` (honours `TDDY_TOOLS_LOG_FILE`) and `run_mcp_server()`
(parses `TDDY_SUBAGENTS_JSON`, serves `PermissionServer` over `rmcp::transport::stdio()`, starts
`session_agents::follow_session_agent_roster`).

**`cli.rs` (927) is far more than arg parsing**: 8 wire-format structs, 6 `#[cfg(unix)]`/`#[cfg(not(unix))]`
relay function pairs, `resolve_submit_goal` policy (exits 2 on `--goal`/payload disagreement), an
output-envelope family, the hard-coded `CLI_SUBCOMMAND_TOOLS` catalog (L511-532), and `run_call_tool`'s
**self-re-exec dispatcher** (L600-662) that maps JSON args back to argv and re-runs `current_exe()`.

#### `src/server.rs` (3,842) — an MCP server, a permission engine, and an MCP→daemon proxy

Protocol: **MCP over stdio via `rmcp` v1**; a real `rmcp::ServerHandler` (L1452-1489) with
`enable_tools()` + `enable_tool_list_changed()`. Secondarily a Claude Code `--permission-prompt-tool`
decision engine (`approval_prompt`, `decide()`, `path_allowed()`, L483-620) that relays undecidable
cases to the TUI over a **bespoke newline-delimited-JSON Unix-socket protocol** (`relay_approve`,
L622-676 — raw `std::os::unix::net::UnixStream`, unlike `toolcall_client`, which was migrated to
`tddy-rpc` framing). Thirdly an MCP→daemon proxy: `dynamic_tool_router` turns any `RemoteToolDef`
catalog into routes forwarding to `session_tool_client::dispatch_session_tool`.

**43 advertisable tools**: 19 static `#[tool]` methods (`approval_prompt`, `github_create_pull_request`,
`github_update_pull_request`, and 16 `pr_*`/`spawn_conversation`); 10 dynamic exec tools
(`Read`, `Write`, `StrReplace`, `Delete`, `Grep`, `Glob`, `Shell`, `Await`, `ReadLints`,
`SemanticSearch`) whose schemas are **hardcoded JSON that must mirror
`tddy_daemon::tool_catalog::tool_catalog()` verbatim** (L1578-1579 names the two guard tests);
3 session-action tools (`request_action`, `list_actions`, `invoke_action`); 6 subagent conversation
tools (`subagent_new_session`/`prompt`/`await`/`cancel`/`list`/`status`); 5 LSP tools behind the
`TDDY_LSP_TOOLS` gate.

**State**: per-instance `PermissionServer { tool_router, socket_path }` (L286-290), plus **three
process-global singletons** — `subagent_sessions()` (`OnceLock<tokio::sync::Mutex<SubagentConversations>>`,
L1855-1858), `session_agents/seed.rs:70` `session_agent_roster()` (`OnceLock<LiveAgentRoster>`), and
`session_tool_client.rs:452` `livekit_room_cache()` (`OnceLock<LiveKitRoomCache>`). Everything else is
env-derived, not held.

**Six natural seams inside `server.rs`:**

| Seam | Lines | ~LoC | What it is | Dependencies |
|---|---|---:|---|---|
| **A. Permission decision engine** | 30-53, 283-292, 483-692 | ~250 | `approval_prompt`, path containment, `decide()`, `relay_approve` | `serde_json` + `std` + env. **Zero tddy deps** |
| **B. PR-stack tool surface** | 55-244, 706-955, 981-1448 | ~1,000 | 14 `pr_*` + `github_*` tools, `to_wire`, `orchestrator_dir`, `repo_slug`, `real_gh`, 8 `*_impl`, 4 `*_json` | `tddy_workflow_recipes::orchestrate_pr_stack::*`, `crate::github_pr`, `crate::toolcall_client` |
| **C. Dynamic tool proxy** | 1491-1682 | ~190 | `RemoteToolDef`, `build_dynamic_tool_list`, `dispatch_dynamic_tool`, `exec_tool_catalog`, `dynamic_tool_router` | `rmcp`, `crate::session_tool_client`, `crate::session_agents` |
| **D. Subagent conversation runtime** | 1684-3061 | ~1,375 | the conversation table, deferred-turn machinery, 6 tools, accounting file, remote-agent opening | `tddy_discovery::subagent`, `tddy_service::proto::connection`, `tddy_core::token_accounting`, `uuid`, `tokio` |
| **E. `ServerHandler` wiring** | 292-480, 1449-1489 | ~230 | `PermissionServer::new()` (router assembly), `advertised_tools`, `call_tool_by_name`, `get_info`, `list_tools` | everything. **The only genuinely "high-level wiring" part** |
| **F. Inline tests** | 3063-4152 | 1,089 raw | 40+ tests spanning A–E | `rstest`, `serial_test` |

#### per-module inventory (LoC = `grep -c .`)

| Module | LoC | prod/test | What it does | Natural destination |
|---|---:|---|---|---|
| `server.rs` | 3842 | 3063/1089 | seams A–F above | split: A→tddy-core/new; B→**tddy-workflow-recipes**; C→**tddy-tool-engine**; D→**tddy-discovery**; E stays |
| `session_tool_client.rs` | 991 | 959/102 | MCP→daemon transport: 4-variant `SessionToolTransport` (SandboxIpc/DaemonHttp/LiveKit/Incomplete), `dispatch_session_tool`, `LiveKitRoomCache`, `MAX_REMOTE_BLOCK_MS = 20_000` | **tddy-service** or a new `tddy-session-tool-client` |
| `cli.rs` | 927 | 678/337 | wire structs, relay pairs, goal policy, output envelopes, self-re-exec | arg structs stay; wire types + relay fns → **tddy-core** |
| `pty_relay.rs` | 843 | 869/69 | 4 dispatch modes (local PTY → `tddy_terminal_rpc::local_pty_relay::run`, gRPC connect-only, gRPC start+connect, LiveKit), GitHub device-flow, `RawMode` termios guard | **tddy-terminal-rpc** (its description already names tddy-tools) |
| `session_agents/registry.rs` | 685 | 685/0 | `LiveAgentRoster` + `RosterState` behind `std::sync::Mutex`, `RosterCurrency`, `CatalogVisibility`, `WithdrawnExecTools`, `Takeover`, `AgentStatus` | **tddy-discovery** or **tddy-service** (cross-cuts) |
| `github_pr.rs` | 451 | 353/143 | PR create/update over REST **by shelling out to `curl`**; `MockGithubTransport`; re-exports from `tddy_workflow_recipes` | **tddy-workflow-recipes** (not tddy-github, which is OAuth-only) |
| `session_agents/stream.rs` | 461 | 461/0 | `StreamSessionAgents` subscription, backoff 500ms→30s, `PASS_LONG_ENOUGH_TO_BE_SERVICE` 30s (read by tddy-daemon tests) | **tddy-service** |
| `action_tools.rs` | 343 | 268/106 | `request_action`/`list_actions`/`invoke_action`; bounded author loop (`MAX_AUTHOR_ATTEMPTS = 3`, `MAX_MANIFEST_BYTES = 64KB`) | **tddy-core** (owns `session_actions::*`) or tddy-actions |
| `remote_cli.rs` | 302 | 302/0 | `remote {list-tools,start-session,connect-session,sync-context}`; reads `daemon.json`, raw `reqwest` POST to `/rpc` | **tddy-service** or tddy-connectrpc |
| `schema.rs` | 292 | 245/87 | `include_dir!("$CARGO_MANIFEST_DIR/../tddy-workflow-recipes/generated")` + `include!(OUT_DIR/goal_registry.rs)`; `get_schema`, `validate_output`, `COMMON_SCHEMAS` | **tddy-workflow-recipes** — *the single most obviously misplaced module* |
| `session_hook.rs` | 275 | 233/84 | Claude Code hook receiver → `ReportSessionStatus` + `ReportAgentActivity`; fail-quiet, always exit 0 | **tddy-core** (owns `activity_status_from_hook`, `parse_hook_event`) |
| `session_agents/conversation.rs` | 271 | 271/0 | `AgentConversationLink`, `RemoteAgentSession` (impls `tddy_discovery::subagent::SubagentSession`), 3 RPCs; refuses truncated streams | **tddy-service** + **tddy-discovery** (split by trait vs wire) |
| `build_cli.rs` | 194 | 194/0 | `build`/`build-list`; `plugin_registry()` registers all 5 build plugins | **tddy-bsp** already owns an identical `plugins::plugin_registry` — a straight duplicate |
| `list_models.rs` | 151 | 134/38 | `list-models --agent <id>` → JSON; owns the daemon⇄tools JSON contract | **tddy-core** (owns `backend::{claude_cli_models, …}`) |
| `restructure_cli.rs` | 147 | 147/0 | `restructure {apply,status,check,anchors,verify}`; `cli_vector()` re-serializes parsed clap args back to strings | **tddy-code-restructuring** — the re-serialization is evidence the boundary is misplaced |
| `lsp_tools.rs` | 137 | 67/92 | 5-tool LSP catalog + `TDDY_LSP_TOOLS` gate; **more test than prod** | **tddy-lsp-executor** |
| `session_actions_cli.rs` | 126 | 126/0 | local fallback for list/invoke-action | **tddy-core** |
| `relay.rs` | 119 | 119/0 | `ensure_relay_daemon`: read `daemon.json`, TCP-probe, else bind + spawn `--relay`, poll 5s. **Zero tddy deps** | **tddy-discovery** or tddy-service; trivially portable |
| `session_agents/seed.rs` | 100 | 100/0 | the spawn seed + `session_agent_roster()` `OnceLock` | with `registry.rs` |
| `analyze_cli.rs` | 86 | 86/0 | `analyze {coverage,report,duplicate-tests}` passthrough | **tddy-code-analysis** |
| `schema_manifest.rs` | 82 | 64/31 | `include_str!("../../tddy-workflow-recipes/generated/schema-manifest.json")` | **tddy-workflow-recipes** |
| `session_agents/link.rs` | 78 | 78/0 | one place to connect to the facilitating daemon | with `session_tool_client` |
| `toolcall_client.rs` | 65 | 65/0 | `dispatch_toolcall` over tddy-rpc/tddy-stdio framing; `method_for()` maps 9 discriminators | **tddy-core** (owns the server side, `toolcall::ToolcallRpcService`) |
| `session_context.rs` | 59 | 59/0 | merge a JSON patch into `{workflow_dir}/{session_id}.session.json`; key cap 256B | **tddy-core** |
| `session_agents.rs` | 44 | 44/0 | module facade: 5 `mod` + 4 `pub use` re-exporting 20 names | with its submodules |
| `lib.rs` | 21 | 21/0 | 14 `pub mod` | stays |
| `main.rs` | 170 | 170/0 | args + dispatch + logging + `run_mcp_server` | **stays — already what tddy-tools should be** |
| `review_persist.rs` | 15 | 15/0 | one function delegating to `tddy_workflow_recipes::review::persist_review_md_to_session_dir` | **tddy-workflow-recipes** — delete on move |

Total **12,354 raw / 10,467 non-blank across 28 files**, ~1,900 of them inline `#[cfg(test)]`.
Also `build.rs` (generates `OUT_DIR/goal_registry.rs` from `tddy-workflow-recipes/goals.json`),
`BUILD.yaml`, and a **second `[[bin]]`** `execute-tool-stdio-fixture`
(`tests/fixtures/execute_tool_fixture.rs`, 49 LoC) whose deps `tddy-rpc`/`tddy-stdio`/`async-trait`
are forced into `[dependencies]` because plain `cargo build` builds it.

#### coupling — three import cycles with `server.rs`

```
cli.rs         → server            cli.rs:545 (advertised_tool_defs), 665 (PermissionServer::new)
cli.rs         → toolcall_client   cli.rs:384, 454, 499, 723, 902, 951
server.rs      → github_pr         :3        → action_tools :319   → lsp_tools :330,331
server.rs      → session_tool_client :301, 1558, 1572, 2029, 2043
server.rs      → session_agents    :362, 369, 419, 1718, 1739, 1753, 2263-2265, 2588, 2666, 2814
action_tools   → server            action_tools.rs:23   ⟵ CYCLE
lsp_tools      → server            lsp_tools.rs:9, 130, 149   ⟵ CYCLE
session_agents/seed   → server      seed.rs:76    ⟵ CYCLE
session_agents/stream → server      stream.rs:217 ⟵ CYCLE
```

The shared items are small — `env_non_empty`, `schema_object`, `subagent_route`,
`subagent_error_json`, `RemoteToolDef`, `seed_subagents_or_report`, `open_roster_agent_session`,
`cancel_remote_conversation` — so **one "shared MCP tool primitives" module breaks all three cycles**.

#### `tddy-tools` external surface — almost nothing

`grep -rn 'tddy_tools::'` outside the crate finds only **4** API surfaces, and **no production code
at all**:

- `session_tool_client::{dispatch_via_sandbox_ipc, dispatch_session_tool}` — tddy-daemon, sandbox-app,
  sandbox-darwin **tests**
- `session_agents::PASS_LONG_ENOUGH_TO_BE_SERVICE` — `tddy-daemon/tests/session_agent_roster_acceptance.rs:596`
- `toolcall_client` — integration-tests (doc reference)

`tddy-daemon → tddy-tools` is **`[dev-dependencies]` only** (`tddy-daemon/Cargo.toml:123`), as are
sandbox-app and sandbox-darwin. Only **`tddy-testing-commons`** has it in `[dependencies]` — and
`tddy-tools` has `tddy-testing-commons` in `[dev-dependencies]`, an existing dev/prod cycle.

#### destination survey (28 packages) — the plausible homes

| Package | src LoC | Owns | Home for displaced code? |
|---|---:|---|---|
| **tddy-core** | 28,953 | 41 pub modules: backend, changeset, claude_hooks, cursor_hooks, agent_activity, session_actions, session_action_{jobs,pipeline}, toolcall, token_accounting, workflow, worktree, presenter, atomic_file | **Yes, strongest.** Already owns the *server* side of `toolcall`, the hook parsers, backend model catalogs, `token_accounting::ConversationRecord`. Risk: already 29k LoC and depended on by ~everything |
| **tddy-service** | 6,255 | "Service definitions and implementations — transport-agnostic"; 17 pub modules + `proto::connection` + an existing `session_agents` module | **Yes.** Owns the exact protos `session_tool_client`/`stream`/`conversation`/`session_hook` speak. Caveat: **depends on `tddy-tui`**, so anything moved here pulls in the TUI |
| **tddy-workflow-recipes** | 19,848 | 21 modules incl. `orchestrate_pr_stack`, `pr_stack`, `plan_pr_stack`, `github_rest_common`, `review`, `schema_pipeline`; owns `goals.json` + `generated/` | **Yes, strongest for schema + PR-stack + github_pr + review_persist.** Already a tddy-tools **build-dependency**, and those modules reach into its directory at compile time |
| **tddy-discovery** | 3,326 | `agent_def`, `subagent` (`SubagentSession`, `SubagentRegistry`, `PromptOutcome`, `CodebaseAccess`, `StopReason`), `openai::TokenUsage`, tools, warmup | **Yes, strongest for seam D + roster.** Already owns every type the subagent runtime manipulates. Needs new deps: `tddy-service`, `uuid`, a transport abstraction |
| **tddy-tool-engine** | 719 | `execute_tool` dispatch for exactly the same 10 tools, path-contained; `catalog::{tool_catalog, ToolDef}` | **Yes, for seam C.** `server::exec_tool_catalog()` is a hand-copied clone guarded by matched tests in both crates; moving it deletes a duplication |
| **tddy-lsp-executor** | 306 | the one concrete `LspExecutor` binding tddy-build target discovery to the tddy-lsp registry; registered by daemon + sandbox-app | **Yes, for `lsp_tools.rs`** — exact fit |
| **tddy-code-analysis** | 1,226 | complexity, coverage, crap, duplicate_tests, report; zero tddy deps | **Yes, for `analyze_cli.rs`**; needs `clap` |
| **tddy-code-restructuring** | 7,824 | apply, backends, edit, journal, ledger, overlay, plan, registry, runner, verify | **Yes, for `restructure_cli.rs`**; needs `clap` |
| **tddy-bsp** | 585 | `bsp.BspService` impl, BUILD.yaml catalog, **`plugins::plugin_registry`** — the same 5-plugin set `build_cli` builds | **Yes, for `build_cli.rs`** — merges a verbatim duplicate |
| **tddy-terminal-rpc** | 1,007 | description already says "shared by tddy-daemon, tddy-coder, and tddy-tools"; owns `local_pty_relay` | **Yes, for `pty_relay.rs`** |
| tddy-github | 1,182 | GitHub **OAuth** only | **No** for `github_pr.rs` — category error |
| tddy-pty | 550 | transport-agnostic PTY spawn + I/O pump + master registry | Partial — `pty_relay` is a *client*, not a PTY host |
| tddy-task, tddy-actions, tddy-lsp, tddy-semantic-index, tddy-graph, tddy-build*, tddy-stdio, tddy-workflow, tddy-codegen, tddy-acp, tddy-testing-commons | 152–1,786 | leaf primitives, plugins, build-time codegen | **No** — wrong altitude, or dep-only |

#### cheapness matrix (does the move add a new Cargo edge?)

| Move | tools / daemon already depend? | Cost |
|---|---|---|
| `schema.rs` + `schema_manifest.rs` + `review_persist.rs` → tddy-workflow-recipes | ✔ / ✔ | **Cheapest.** No new edges; also removes tddy-tools's `include_dir`/`include_str`/`build.rs` reach into a sibling package |
| seam B (PR-stack) + `github_pr.rs` → tddy-workflow-recipes | ✔ / ✔ | **Cheap**, ~1,450 LoC; target already owns `orchestrate_pr_stack` |
| `analyze_cli.rs` → tddy-code-analysis | ✔ / ✘ | Cheap; add `clap`+`anyhow` |
| `restructure_cli.rs` → tddy-code-restructuring | ✔ / ✘ | Cheap; add `clap` |
| `build_cli.rs` → tddy-bsp | ✘ (new edge) / ✔ | Medium — merges a duplicated registry and lets tddy-tools drop **all 6** tddy-build* deps |
| seam C (exec catalog) → tddy-tool-engine | ✘ (new edge) / ✔ | Medium — deletes a hand-copied catalog and its two guard tests |
| `lsp_tools.rs` → tddy-lsp-executor | ✘ (new edge) / ✔ | Cheap (137 LoC, 92 tests) |
| `toolcall_client`, `session_context`, `session_actions_cli`, `session_hook`, `list_models`, `cli.rs` wire types → tddy-core | ✔ / ✔ | Cheap edge-wise; ~630 LoC; grows tddy-core |
| seam D + `session_agents/registry.rs` → tddy-discovery | ✔ / ✔ | **Largest single move**: ~2,050 LoC + ~4,500 LoC of tests |
| `session_tool_client` + `session_agents/{stream,conversation,link,seed}` → tddy-service | ✔ / ✔ | Medium-large (~1,900 LoC). **Would let tddy-daemon, sandbox-app and sandbox-darwin drop their dev-dep on tddy-tools entirely** |
| `pty_relay.rs` → tddy-terminal-rpc | ✔ / ✔ | Medium (843 LoC) |
| `relay.rs` → tddy-discovery/tddy-service | ✔ / ✔ | Cheap — 119 LoC, zero tddy deps |
| seam A (permission engine) → tddy-core or new crate | ✔ / ✔ | Cheap edge-wise (~250 LoC), but carries a bespoke NDJSON socket protocol |

#### `tests/` — 50 files, 12,158 LoC

Heaviest: `session_agent_roster_client_acceptance.rs` 1040, `subagent_async_response_acceptance.rs` 671,
`request_action_mcp_acceptance.rs` 569, `session_agent_conversation_client_acceptance.rs` 458,
`subagent_status_wait_acceptance.rs` 456, `session_tool_livekit_dispatch.rs` 437,
`subagent_token_accounting_acceptance.rs` 419, `actions_cli_acceptance.rs` 398,
`subagent_mcp_acceptance.rs` 374, `session_action_jobs_acceptance.rs` 343, `cli_integration.rs` 336,
`schema_validation_tests.rs` 327, `remote_cli_subcommand_acceptance.rs` 287.

**Test weight by target**: `server.rs` seam D (subagents) ≈ **3,000 LoC**; `session_agents/*` ≈ 1,500;
`session_tool_client` ≈ 1,010; `cli.rs`+`main.rs` ≈ 1,100; `toolcall_client` ≈ 267. Several drive the
**real `--mcp` stdio wire** via `assert_cmd`, so they move with the seam rather than being rewritten.

#### `Cargo.toml`

17 workspace + 17 external deps. Workspace: tddy-rpc, tddy-stdio, tddy-terminal-rpc, tddy-core,
tddy-discovery, tddy-sandbox, tddy-build ×6, tddy-code-analysis, tddy-code-restructuring, tddy-lsp,
tddy-task, tddy-workflow-recipes, tddy-service, optional tddy-livekit. External: rmcp 1, clap 4,
tokio 1, reqwest 0.12, jsonschema 0.44, prost 0.13, include_dir 0.7, schemars 1, uuid, bytes, serde,
serde_json, anyhow, tempfile, log, env_logger, async-trait. `default = ["livekit"]`.

### Findings

1. **`main.rs` is already the shape the brief asks for; `lib.rs`'s 14 modules are the problem.**
   The thinning is a library-surface question, not a CLI question.
2. **`server.rs` has six clean seams, and five of them have an existing owner** — B→tddy-workflow-recipes,
   C→tddy-tool-engine, D→tddy-discovery, plus `lsp_tools`→tddy-lsp-executor and `github_pr`→
   tddy-workflow-recipes. Only seam A (the permission engine, zero tddy deps) has no obvious home.
3. **Three import cycles with `server.rs` must be broken first** — `action_tools`, `lsp_tools`,
   `session_agents/{seed,stream}` all import back from it, over eight small shared items. One shared
   primitives module fixes all three, and it is a prerequisite for every `server.rs` seam move.
4. **`tddy-tools` has essentially no production consumers** (4 surfaces, all from tests; 4 of 5
   reverse-deps are dev-deps). Moving code out of it is therefore cheap at the caller boundary — the
   opposite of the daemon's situation.
5. **Three moves delete duplications rather than relocating them**: `build_cli::plugin_registry` vs
   `tddy_bsp::plugins::plugin_registry` (verbatim), `server::exec_tool_catalog` vs
   `tddy_tool_engine::catalog::tool_catalog` (hand-copied, guarded by matched tests), and
   `github_pr.rs` re-exporting from `tddy_workflow_recipes::github_rest_common`.
6. **Moving `session_tool_client` + `session_agents/*` into tddy-service lets three crates drop their
   dev-dep on `tddy-tools` entirely** — a measurable, reviewable outcome for one node.
7. **`tddy-service` depends on `tddy-tui`**, so it is not a free destination: anything moved there
   pulls the TUI into every consumer's build.
8. **`tddy-testing-commons` ↔ `tddy-tools` is an existing prod/dev cycle** that any change to
   tddy-tools's public surface touches first.
9. **25 env vars are the crate's real hidden interface** (`TDDY_SOCKET` ×43, `TDDY_REPO_DIR` ×24).
   A cross-crate move must carry the env contract, not just the code.

## Exploration 4: what `tddy-tools restructure` can and cannot do — 2026-09-09

**Agent**: Explore subagent
**Scope**: the `code-restructuring` skill and its `plan-schema.md`; the `tddy-code-restructuring`
implementation; the pivotal question of **cross-crate movement**; preconditions, failure modes, the
practical workflow; whether any other tool in the repo rewrites import paths.

### Sequence

1. `find .agents/skills/code-restructuring -type f`; `find packages/tddy-code-restructuring/src -name '*.rs'` — the skill's 3 files and the crate's 13 modules.
2. Read `SKILL.md` (45 lines) and `references/plan-schema.md` (205 lines) in full.
3. `grep -n "serde(rename" plan.rs`; `grep -n "^pub enum"` — locate the wire tags.
4. Read `plan.rs:1-320` — quote `RefactorKind`, `Reexport`, `RefactorOp`, and the parser's refusals.
5. Read `packages/tddy-tools/src/restructure_cli.rs` in full — CLI wiring, when an LSP client is required.
6. `grep -n "RefactorKind::" backends/rust.rs`; read `rust.rs:1-330` — the `SUPPORTED` array and the assist table.
7. **`grep -rn "Cargo.toml\|cargo\|crate_root\|package\|manifest\|workspace_root\|cross-crate\|other crate" src/`** — the key question: any manifest/package awareness at all.
8. `grep -rn "unsupported\|refuse"`; read `registry.rs:1-130` — how a backend/destination is resolved.
9. Read `lib.rs` — the complete error vocabulary.
10. Read `rust.rs:928-1307`, `1307-1406`, `1542-1800` — `resolve`, `edit_for`, `reach_of`, `rename_symbol`, `ensure_indexed`.
11. Read `apply.rs` (373) and `verify.rs` in full — the only filesystem writer, and the verification unit.
12. Read `runner.rs:1-620` — subcommands, root determination, `.restructure/` state.
13. Read `docs/ft/coder/rust-code-restructuring.md` in full — preconditions and known limitations.
14. `git log --all --oneline --grep=restructure`; `git show --stat 0772dd9d` + full diff — the "stop committing restructure plans" commit, and **five real plan files captured verbatim**.
15. Read `.agents/skills/code-restructuring/references/restructure-changeset.md` (208) — the changeset contract and the moved-line diff.
16. `grep -rn "move item to file\|move module to file\|codemod\|rewrite_import\|rewrite imports"` over `packages scripts .agents docs` — is there ANY cross-crate tooling.
17. `for c in <6 commits>; git merge-base --is-ancestor $c HEAD` — **are the four bridge fixes in the current tree?**
18. `grep -rn "set_request_timeout\|with_capabilities\|with_initialization_options"`; read `tddy-lsp/src/client.rs:90-150,290-400` and `error.rs` — confirm the defect state from code.
19. Read `.agents/skills/analyze-code-issues/SKILL.md`; `grep -n "enum AnalyzeCommand" analyze_cli.rs`.

### Grep / glob

| Pattern | Scope | Result |
|---|---|---|
| `Cargo.toml\|cargo\|crate_root\|package\|manifest\|workspace_root\|cross-crate\|different crate` | `tddy-code-restructuring/src/**` | **17 hits, all irrelevant to destinations** — only rust-analyzer env pinning (`CARGO_HOME`/`CARGO`/`RUSTC`), one test writing a fake `cargo`, two doc words. **Zero manifest reads, zero package resolution, zero `Cargo.toml` writes.** |
| `op\.to\b` / `with_private_deps` | `backends/rust.rs` | **Zero hits.** `RefactorOp::to` and `with_private_deps` are never read on the Rust path |
| `UnsupportedOp\|MoveSymbol\|MoveFile` | crate src | `registry.rs:124`, `rust.rs:1032` (constructed); `plan.rs:53,55` (enum); `registry.rs:212` (test: `MoveSymbol` on a `.rs` file → `UnsupportedOp`) |
| `move item to file\|move to file\|codemod\|rewrite_import` | `packages`, `scripts`, `.agents`, `docs` | **2 hits, both negative**: `plan.rs:40` ("`rewrite_import_path` **were dropped** for having no engine behind them") and `plan.rs:52` ("rust-analyzer has no whole-symbol move"). **No codemod exists in this repo** |
| `codeAction\|code_action\|assist` | `tddy-lsp/src`, `tddy-lsp-executor/src` | **Zero hits.** `tddy-lsp` has no typed assist API — only `request_raw`/`notify_raw`. rust-analyzer assists are reachable **only** through `tddy-code-restructuring` |
| `LSP_TOOL_NAMES` | `tddy-tools/src/lsp_tools.rs:16` | 5 read-only tools; **no assist/refactor tool is exposed to agents over MCP** |
| `sed -i` / `s/crate::` | whole repo scripts | one unrelated hit in `.vscode/scripts`. **No path-rewriting script** |
| `set_request_timeout\|with_capabilities\|with_initialization_options` | tddy-lsp, tddy-tools, tddy-code-restructuring | **Zero hits in the current tree** — these are the D3/D4 fixes, present only on the base branch |
| `git merge-base --is-ancestor` | 6 restructure commits vs `HEAD` | `202ced03` (original feature) **YES**; `28fa278d`, `be98e4c1`, `0af927b4`, `3d264466`, `0772dd9d` **NO** |

### Inspected files

#### `plan-schema.md` — the operation table (verbatim)

| `op` | Anchor | Fields | TypeScript | Rust |
|---|---|---|---|---|
| `extract_method` | range | `name`, `variant` = `inner`\|`module` | ✅ | ✅ |
| `extract_variable` | range | `name` | ✅ | ✅ |
| `extract_type` | range | `name`, `variant` | ✅ | — |
| `extract_class` | range over whole members | `name`, `to` | ⚠️ | — |
| **`move_symbol`** | symbol | `to`, `with_private_deps` | ✅ | **—** |
| **`move_file`** | symbol | `to` | ✅ | **—** |
| `rename_symbol` | symbol or range | `name` | ✅ | ✅ |
| `extract_module` | range over a selection of items | `name`, `reexport` = `glob`\|`named`\|`none`, `to_file` | — | ✅ |
| `extract_module_to_file` | range at the `mod` keyword | — | — | ✅ |
| `extract_trait` | range at the `impl` keyword | `name` | — | ✅ |
| `inline_method` | symbol | — | — | ✅ |
| `organize_imports` / `add_missing_imports` | symbol | — | ✅ | — |

The decisive line (`plan-schema.md:52-54`):

> **Rust has no whole-symbol move.** To split a Rust file, group the items with `extract_module` and
> then run `extract_module_to_file`, whose anchor is a caret on the `mod` keyword.

#### `packages/tddy-code-restructuring/src/backends/rust.rs:37-45` — what Rust actually supports

```rust
const SUPPORTED: [RefactorKind; 7] = [
    RefactorKind::ExtractMethod,
    RefactorKind::ExtractVariable,
    RefactorKind::ExtractModule,
    RefactorKind::ExtractModuleToFile,
    RefactorKind::ExtractTrait,
    RefactorKind::InlineMethod,
    RefactorKind::RenameSymbol,
];
```

#### `plan.rs:34-96` — the enum, with the authors' own verdict in the doc comments

```rust
pub enum RefactorKind {
    ExtractMethod, ExtractVariable, ExtractType,
    /// TypeScript `Move to file`. rust-analyzer has no whole-symbol move.
    MoveSymbol,
    /// TypeScript, which rewrites every importer as part of the move.
    MoveFile,
    RenameSymbol, ExtractModuleToFile, OrganizeImports, AddMissingImports,
    ExtractClass, ExtractModule, ExtractTrait, InlineMethod,
}
```

> `inline_symbol` and **`rewrite_import_path` were dropped for having no engine behind them**; a
> vocabulary that advertises what cannot be performed is worse than a smaller one. — `plan.rs:40`

> **rust-analyzer has no "move item to another module" assist**, so there is no engine to delegate
> the facade to and this package authors the `use` line itself. — `plan.rs:79-80`

#### `registry.rs:17-31` — the only notion of "root" in the whole crate

```rust
pub struct Workspace<'a> {
    pub root: &'a Path,
    pub overlay: &'a Overlay,
}
```

Dispatch is by **file extension only** (`registry.rs:33-80`); an unsupported op is
`UnsupportedOp { backend, op }` — "an error, never a silent skip".

#### `rust.rs:1616-1621` — the three in-place ops write exactly one file

```rust
Ok(WorkspaceEdit { changes: vec![FileEdit::Change {
    path: produced.relative.to_string(),
    edits: minimal_edits(produced.original, produced.text),
}] })
```

`produced.relative` is `op.anchor.file()`. A destination in another crate is not expressible.

#### `rust.rs:87-93` — a file *move* is refused at the conversion layer

```rust
match change.get("kind").and_then(Value::as_str) {
    Some("create") => { … Ok(vec![FileEdit::Create { path }]) }
    Some(other) => Err(failure(format!("unsupported resource operation `{other}`"))),
```

The client advertises `resourceOperations: ["create","rename","delete"]` but honours only `create`.
`FileEdit::Rename` exists and `apply.rs` would `git mv` it anywhere in the repo — **but nothing on
the Rust path ever emits one.**

#### `rust.rs:1698` + `2637-2639` — `rename_symbol` cannot re-point callers either

`RenameSymbol` is not in `assist_for`, so it takes the single-document path, and `edits_for`
**filters `documentChanges` down to the one `uri`**. rust-analyzer's rename *does* compute cross-file
edits; **this backend throws every other file's edits away**. A rename of a symbol referenced
elsewhere silently breaks those callers rather than updating them.

#### `verify.rs:1-14` — the verifier states the intra-crate assumption, but is mechanically repo-wide

```rust
//! Whole-crate rather than per-seam on purpose: a restructure moves code *within* a crate, so the
//! crate is the unit over which the multiset is invariant, and no knowledge of the seams is needed.
```

Mechanically `sources_at`/`sources_now` (`runner.rs:487-525`) walk `git ls-tree -r <ref>` and
`git ls-files` from `current_dir`, keep every `*.rs` outside `target/`, and compare only totals —
"Paths are not compared." **So `verify --against` run from the repo root does validate a cross-crate
move.**

#### the `reexport` facade and the visibility pass

- `"glob"` → one line `pub use <module>::*;`. "legal whatever moved: a glob re-export caps at each
  item's own visibility rather than failing on a member less visible than itself." Also carries nested modules.
- `"named"` → one grouped `use` per visibility tier, widest first, naming only items reached from
  outside. Refuses (`refuse_uncovered_nesting`) when something outside reaches a nested item the
  facade would not carry.
- absent / `"none"` → no facade, and `refuse_stranded` fires for any item referenced from another file.

`restore_visibility` (`rust.rs:3573-3630`): the assist rewrites everything it relocates to
`pub(crate)`; the pass puts back the original visibility wherever nothing outside needs it widened
and **reports every widening that has to stand**. `impl_widenings` reports but deliberately does
**not** restore relocated `impl` members — "**expect a private method to come out `pub(crate)` and
stay there**".

#### `restructure_cli.rs:83-120` — preconditions

```rust
let client = if needs_lsp {
    let root = std::env::current_dir().context("current_dir")?;
    let lsp_registry = LspRegistry::new(LspAllowList::rust_only(), task_registry, Duration::from_secs(600));
    let key = LspKey { root, language: Language::Rust };
    Some(Arc::clone(&lsp_registry.get_or_spawn(key).await?.client))
} else { None };

fn needs_lsp_client(args: &[String]) -> bool {
    match args.first().map(String::as_str) {
        Some("apply") | Some("anchors") => true,
        Some("check") => args.iter().any(|a| a == "--deep"),
        _ => false,
    }
}
```

The LSP client is rooted at `std::env::current_dir()`, so **`tddy-tools restructure` must be run from
the repo root**. Git worktree is mandatory (`NotAGitWorktree`); the snapshot is re-hashed
(`SnapshotMismatch`); one journal per plan under `.restructure/` (`JournalExists`, and
`rm -rf .restructure/` is required between plans that anchor on text the previous plan produced);
a green baseline is required — "a red tree is a stop".

#### the four bridge defects are NOT in the current tree

| Defect | Current-tree evidence | Effect |
|---|---|---|
| D1 JSON-RPC `error` discarded | `tddy-lsp/src/client.rs:395-397`: `let result = message.get("result").cloned().unwrap_or(Value::Null);` — no `error` branch; `error.rs` has no `Server` variant | every server error arrives as a successful **empty** answer; the `ContentModified` retry loop is dead |
| D2 everything is a plan defect | `backends/lsp_bridge.rs:38-40`: `map_lsp_error` → `MalformedPlan` unconditionally | a slow index is reported as a defective plan; `ServerCatchingUp` is never produced |
| D3 `--indexing-budget` inert | `tddy-lsp/src/client.rs:20`: `const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);` — no `set_request_timeout` anywhere | every request hard-capped at 10 s |
| **D4 (the blocker)** | `client.rs:121-125`: `"capabilities": {}`; `RustBackend::start()` returns early when a bridge exists, so `client_capabilities()` is **never sent** on the CLI path | rust-analyzer returns **no code actions at all**; server stays on utf-16 while this backend counts utf-8 bytes — **every column silently wrong on any line with a non-ASCII character** (this repo's comments are full of em dashes) |

All four present as the same false message: `rust-analyzer offers no "extract into function" assist
for the given range` (`rust.rs:910-912`). The fixes exist **only** on
`feature/connection-service-split/lsp-settle-budget`.

#### plans are execution artifacts, not documentation

Commit `0772dd9d` deleted five committed `plan-*.jsonl` files:

> A plan is an execution artifact, not documentation. Its header pins one `sha256` of one file, so it
> is unreplayable the moment the next plan runs and unreadable afterwards as a record of intent.

A real plan line, verbatim:

```jsonl
{"v":1,"snapshot":{"packages/tddy-daemon/src/connection_service.rs":"sha256:162ba34b…"}}
{"op":"extract_module","anchor":{"kind":"range","file":"packages/tddy-daemon/src/connection_service.rs","start":{"line":128,"col":1},"end":{"line":304,"col":2}},"name":"service_util","to_file":true,"reexport":"glob"}
```

#### the impl geometry table — the one unfixable refusal

| Geometry | Outcome |
|---|---|
| A whole `impl` moves; the parent calls its methods | **Succeeds** — a method is reached through its type, nothing to rewrite |
| A path-reached item moves; the parent still names it | **Succeeds** — the assist rewrites the reference, the import pass restores the binding |
| **One member is lifted out of an `impl` while a sibling in that same `impl` calls it** | **Refused** — the new module is written *outside* the impl, so the rewritten path never resolved from in there |

"no ordering fixes it: an `impl` body cannot hold a `mod`… Grow the seam to carry the whole `impl`,
or cut it where nothing crosses." And `rust.rs:1282-1291`: relocating a whole `impl` "leaves every
caller resolving, **in this crate and in any other**. That is verified, not assumed: a `pub` method in
a private module builds from a separate crate."

#### consequences that are not refusals but will bite

- **Unused imports left in the parent after every `extract_module` seam.** "nothing prunes the
  parent's now-unused `use` declarations… `cargo clippy -- -D warnings` will fail until they are
  pruned; the pruning is deletion, not authoring."
- **Successive `extract_method`s accumulate into ONE new inherent `impl` block**, and "**there is no
  operation that splits an `impl` block**" — boundaries (`}` + `impl Foo {`) are inserted by hand,
  which was "the only hand-written lines in the restructure", verified as a zero-moved-line diff.
- Restructuring tests that start rust-analyzer are load-sensitive — run with `--test-threads=1`.

#### `tddy-tools analyze` (targeting)

`coverage` / `report` / `duplicate-tests`; `CRAP = complexity² × (1 − coverage)³ + complexity`; join
key is `(file, declaration line)`, never function name. Requires the nix dev shell. **It is per-crate**,
so it answers *which code inside `tddy-daemon`/`tddy-tools` is worst*, **not** *which crate code
belongs in* — the destination question is a design judgement it cannot answer.

### Findings

1. **VERDICT: no operation can move an item from one crate to another.** Five independent proofs:
   `MoveSymbol`/`MoveFile` are absent from `SUPPORTED` (hard `UnsupportedOp`); `RefactorOp::to` is
   read nowhere on the Rust path; `edit_for` emits exactly one `FileEdit::Change` for the anchor's own
   file; `extract_module_to_file` names and places the file itself, always beside the parent; and
   `rewrite_import_path` was **deliberately removed** from the vocabulary for having no engine.
   Nothing in the crate reads or writes a `Cargo.toml`.
2. **`rename_symbol` is a trap, not a workaround.** The backend filters rust-analyzer's cross-file
   rename edits down to the anchor's own document, so it would break external callers silently.
3. **No other tool in the repo does this either** — no codemod, no import rewriter, no `cargo`-aware
   move; rust-analyzer's assists are reachable only through this crate, and the agent-facing MCP LSP
   tools are five read-only queries.
4. **But the tool makes the crossing mechanical, and at proven scale.** `extract_module` +
   `to_file: true` + `reexport: "glob"` clusters scattered items into one cohesive file with **zero
   caller diff** — demonstrated over 23,099 lines and 93 referring files. That file is then a
   single-file unit ready for `git mv`.
5. **Therefore every node has the same three-phase shape**: **A)** tool-only clustering inside the
   source crate (`extract_method` → hand-inserted `impl` boundaries → `extract_module`+`to_file`+`glob`,
   definitions-first, one plan at a time, `check --deep` → `--dry-run` → apply → `verify --against HEAD`
   → prune stale imports → commit → `rm -rf .restructure/`); **B)** the boundary crossing by hand but
   fully mechanical (`git mv`, create the crate, one line in `[workspace].members`, dependency lines,
   fix the moved file's own `use` header, then either rewrite callers or leave
   `pub use tddy_<new>::…;` in the old crate and change **no caller at all**); **C)** the obligations
   the tool creates (`pub(crate)` widenings that must stand, stale-import pruning, doc triage).
   `restructure verify --against <pre-move-ref>` from the repo root still validates phase B.
6. **`refuse_stranded` is a free cross-crate blast-radius report.** `reach_of` counts any reference
   whose URI differs from the anchor's — other crates included — so running `check --deep` with
   `reexport` **omitted** makes the tool name every externally-reached item and every referring file.
   That is the caller inventory each node needs, obtained mechanically.
7. **Basing on `feature/connection-service-split/lsp-settle-budget` is a hard requirement, not a
   preference.** All four bridge defects are still present on `master` and on the current branch;
   without them `apply` fails on every operation with a message that reads like a plan defect. This
   independently confirms the user's chosen base.
8. **Moving a whole `impl` is the cheapest move Rust has**, and it is verified to keep callers
   resolving across a crate boundary — so seams should be cut at `impl` granularity wherever possible.

## Exploration 5: `tddy-daemon` subsystem map, adjacency graph and immovable core — 2026-09-09

**Agent**: Explore subagent
**Scope**: what the daemon's "wiring" is today; the 106 flat modules grouped into cohesive
subsystems with exact LoC and their test files; per-subsystem coupling to `ConnectionServiceImpl`,
to the proto packages and to each other; the knot that cannot move; the dependency cycles that
block a crate split; `Cargo.toml`; test organisation.

### Sequence

1. `ls -1 *.rs | wc -l` + `grep -c .` per file in `src/` — 106 flat `.rs` (not 108), 82,599 non-blank total.
2. `ls -1F | grep '/'` + `cat -n lib.rs` — only `model_registry/` is a subdir; the verbatim `mod` list.
3. `cat -n main.rs server.rs startup.rs` — the three small wiring files in full.
4. `cat -n src/runtime.rs` + `cat -n Cargo.toml` — the assembler and the dependency list.
5. `grep -n '^pub struct\|^impl \|^pub type' runtime.rs`; `sed -n '130,330p'`, `'323,410p'`, `'406,700p'`, `'700,1010p'` — the whole of `build()`.
6. `grep -n '^pub struct ConnectionServiceImpl' -A 200` — the 60-field struct.
7. `grep -rn "connection_service::[A-Za-z_]*"` per file, then again excluding comment lines — **adjacency v2, the authoritative graph**.
8. `grep -n "#\[cfg(test)\]" connection_service.rs`; `^impl `; `^mod ` — 41 impl blocks, 29 **interleaved** test mods.
9. `grep -n "pub fn new(" -A 40`; `self_arc\|set_self_handle`; `pub fn with_[a-z_]*(` — the 8-arg ctor, the `Weak` self-handle, the 21 builders.
10. `grep -rn "^pub trait"` over all src — 30 trait ports.
11. `grep -rn "tddy_service::proto::[a-z_]*"` — proto package per file.
12. `grep -rln "tddy-daemon" --include=Cargo.toml packages/` then `grep -rn "tddy_daemon::"` in each consumer — the external API surface.
13. `comm -23` module list vs referenced modules — in-degree-zero modules → **found unwired `vnc_service`, `tool_catalog_sync`, `codex_oauth_relay`**.
14. `awk` state machine on `^#[cfg(test)]` → `^}` — precise prod vs inline-test LoC.
15. Grouped `grep -c .` over explicit file lists — subsystem tables, **verified to sum to 82,599 exactly** (no file missed, none double-counted); test tables sum to 58,174 exactly.
16. `awk` in-degree computation over adjacency v2 — core vs leaves.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|---|---|---|---|
| grep -rn | `connection_service` minus comment lines | `src/*.rs` minus self | **Real code hits in only 8 files**: `test_util.rs:10`, `cursor_cli_spawn.rs:20,43,136,155,245,312,416`, `sandbox_session.rs:197,225,262,364,999,1025,1048,1086`, `session_agent_inference.rs:36`, `context_files.rs:41`, `telegram_session_subscriber.rs:122`, `auth.rs:25`, `runtime.rs` (3). **Every other hit is a doc comment** (14 more files) |
| grep -n | `session_room` | `config.rs` | **`config.rs:85: crate::session_room::DEFAULT_GIT_TIMEOUT.as_millis() as u64`** — the only wiring→subsystem code edge |
| grep -n | `crate::split_session` | `livekit_peer_discovery.rs` | **`529: id_trim.starts_with(crate::split_session::SPLIT_AGENT_IDENTITY_PREFIX)`** — a single `&str` const |
| grep -n | `#[cfg(test)]` | `connection_service.rs` | 29 hits, first at **8403**, last at 23592 — interleaved with prod code, not a clean tail |
| grep -rn | `^pub trait` | `src/**` | **30 traits** already defined |
| grep -rn | `tddy_service::proto::` | `src/**` | 16 proto packages; **`connection` alone is referenced by 20 files** |
| grep -rln | `tddy-daemon` | `packages/*/Cargo.toml` | **3 real library dependents**: `tddy-desktop/src-tauri`, `tddy-sandbox-app`, `tddy-integration-tests` |
| grep -rn | `vnc_service\|VncService` | `packages/` | only `lib.rs:111`, the two source files, two test files. **Never registered in `runtime.rs`** |
| grep -rn | `tool_catalog_sync` | `packages/` | only `lib.rs:93` |
| grep -l | `connection_service` | `tests/*.rs` | **80 of 166** test files |
| awk | cfg(test) state machine | `src/**` | **prod 63,353 / inline test 19,246 / total 82,599** |

### Inspected files

#### `src/lib.rs` (109) — and an extraction precedent

100 `mod`/`pub mod` entries; only two are private (`codex_oauth_participant_metadata`,
`oauth_loopback_tunnel`). Lines 100-102 record how an earlier extraction stayed source-compatible:

```rust
// Re-export the shared tool engine so legacy `crate::tool_engine::...` references inside the
// daemon keep resolving after the extraction into the `tddy-tool-engine` crate.
pub use tddy_tool_engine as tool_engine;
```

**Three subsystems were already extracted this way** — `tddy-task` (via `relay_idle.rs`, 6 lines),
`tddy-pty` (via `pty_registry.rs`, 5 lines), `tddy-tool-engine` (via the alias above). Each shim
costs 5–7 lines.

#### `src/main.rs` (168) — genuinely thin already

`libc::signal(SIGPIPE, SIG_IGN)` → clap parse of `Args { config: Option<PathBuf> (env
TDDY_DAEMON_CONFIG), relay: bool }` → `DaemonConfig::load` → `tddy_core::init_tddy_logger` →
**pre-tokio fork** of the spawn worker (`supervisor_client::spawn_backend_choice` →
`spawn_worker_for`) → `set_git_ssh_command` → `startup::startup_config_check` → build a multi-thread
tokio runtime and `block_on(runtime::build(config, RuntimeOptions::for_binary()…))` → a SIGTERM task
calling `daemon.cli_sessions.kill_all()` → `server::run_server(12 args)` → `kill_all` +
`tasks.abort_all()`.

**Env overrides are not in `main.rs`** — they are `runtime::apply_env_overrides`.

#### `src/runtime.rs` (1089 = 1041 prod / 48 test) — this *is* the wiring

```rust
pub enum RuntimeHost { Binary, Embedded }                                  // :35

pub struct RuntimeOptions {                                                // :46
    pub host: RuntimeHost, pub relay_idle_timeout: Option<Duration>,
    pub oauth_redirect_uri: Option<String>,
    pub spawn_client: Option<(crate::spawn_worker::SpawnClient, i32)>,
    pub config_path: Option<PathBuf>,
}

/// An assembled daemon: every service it hosts, plus the configuration it was built from.
pub struct DaemonRuntime {                                                 // :139
    pub entries: Vec<tddy_rpc::ServiceEntry>, pub config: DaemonConfig,
    pub cli_sessions: Arc<crate::cli_session_manager::CliSessionManager>,
    pub lifecycle_telegram: Option<(DaemonConfig, Arc<dyn TelegramSender + Send + Sync>)>,
    pub relay_shutdown: Option<tokio::sync::oneshot::Receiver<()>>,
    pub tasks: RuntimeTasks,
}

/// The listeners, dialers and background loops an assembled runtime needs, none of them started.
pub struct RuntimeTasks { common_room, oauth_loopback_tunnel, local_socket,   // :168
                          lsp_idle_reaper, relay_idle_monitor, telegram_inbound }
```

Its doc contract (lines 10-14): *"[`build`] is assembly: it derives every service from configuration
and returns the handles. **Nothing that listens, dials or runs forever is started there.**"*

`build()` registers **14 RPC entries in order**: `auth::build_auth_entries` →
`build_token_service_entry` → `connection.ConnectionService` → `models.ModelRegistryService` →
`AcpServiceServer` → `tasks.TaskService` → `remote_git.RemoteGitService` →
`session_admission.SessionAdmissionService` → `actions.ActionService` → `bsp.BspService` →
`vm.VmService` (impl from `tddy_vm`, not the daemon) → `screen_sharing.ScreenSharingService` →
`DaemonConfigService` → `reflection_entry_from`. **Entries 3–12 are gated on
`if let Some(user_resolver) = auth_result.user_resolver`** — no GitHub config means no session
services at all.

`apply_env_overrides` (:323) handles `LIVEKIT_PUBLIC_URL`, `LIVEKIT_URL`, `LIVEKIT_API_KEY`,
`LIVEKIT_API_SECRET`, `WEB_HOST`, `WEB_PUBLIC_URL`, `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`,
`GITHUB_REDIRECT_URI`, `TDDY_DAEMON_INSTANCE_ID`, then delegates to three `config.apply_*` methods;
plus `TDDY_DATA_DIR` in `tddy_data_dir_for`.

The six **resolver closures** — the real cross-cutting abstraction, and note each subsystem
**already defines its own resolver type alias**:

```rust
let sessions_base_resolver: crate::connection_service::SessionsBaseResolver = …;
let projects_dir_resolver:  crate::remote_git_service::ProjectsDirResolver  = …;
let bsp_session_resolver:   crate::bsp_service::SessionPathsResolver        = …;
let chat_workspace_roots:   crate::model_registry::ChatWorkspaceRoots       = …;
let ss_sessions_base:       crate::screen_sharing_service::SessionsBase     = …;
let uid_to_username:        crate::connection_tonic_adapter::UidToUsername  = …;
```

The self-handle wiring (~:735):

```rust
let connection_arc = Arc::new(connection_impl);
connection_arc.set_self_handle(Arc::downgrade(&connection_arc));
let task_registry      = connection_arc.task_registry();
let session_admissions = connection_arc.session_admissions();
```

`task_registry` then goes to `tddy_lsp_executor::register`, `TaskServiceImpl`, `ModelAcpService`,
`ActionServiceImpl` and `tddy_vm::VmServiceImpl` — but **it actually originates in
`CliSessionManager`** (`connection_service.rs:2049`) and is merely re-exposed through
`ConnectionServiceImpl`.

#### `src/server.rs` (98) and `src/startup.rs` (31)

`run_server` — 12 positional args, `#[allow(clippy::too_many_arguments)]`. Sends the Telegram
"started" lifecycle message, `MultiRpcService::new(rpc_entries)` →
`tddy_connectrpc::connect_router(RpcBridge::new(multi))` → `tddy_coder::web_server::ClientConfig` →
`serve_web_bundle_with_shutdown(…)`; `shutdown_signal()` = `ctrl_c` ∪ SIGTERM ∪ optional relay-idle
oneshot. `startup_config_check` is one pure fn requiring `listen.web_port`, and `web_bundle_path`
unless relay.

#### `src/config.rs` (2204 = 1268 prod / 936 inline test)

18 public config structs. `DaemonConfig` has 25 fields. **In-degree 29 — the highest in the crate**;
every subsystem reads it. Its **only** outbound code edge is `config.rs:85`.

#### `src/connection_service.rs` — 23,099 non-blank (17,151 prod + 5,948 inline test)

41 `impl` blocks; 29 interleaved test mods. The struct's own contract:

```rust
/// `Clone` is a shallow, shared clone: every mutable field is behind an `Arc`, so a clone talks to
/// the same session managers, registries and caches. The server-streaming handlers need it — they
/// hand the work to a `tokio::spawn`ed producer task, which must own a `'static` service.
#[derive(Clone)]
pub struct ConnectionServiceImpl { /* 60 fields */ }

pub type SessionUserResolver  = Arc<dyn Fn(&str) -> Option<String>  + Send + Sync>;   // :236
pub type SessionsBaseResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;   // :239

/// Self-reference for handing out `Arc<ConnectionServiceImpl>` from a `&self` method. Set once
/// right after the top-level `Arc::new` in `runtime.rs` …
self_handle: Arc<std::sync::OnceLock<std::sync::Weak<ConnectionServiceImpl>>>,          // :1472
pub fn self_arc(&self) -> Arc<ConnectionServiceImpl> {
    self.self_handle….expect("ConnectionServiceImpl::self_arc called before set_self_handle")
}
```

`self_arc()` is called at **6834, 7295, 8136** — three `conn: self.self_arc()` sites feeding the
sandbox-IPC `DaemonRpcHandler`. (This is the source of the one recorded pre-existing test failure.)

Constructor: **8 positional args plus 21 `with_*` builders** — the whole test-injection surface
(`with_model_registry`, `with_session_rooms`, `with_staging_base_dir`, `with_github_token_store`,
`with_session_notification_bus`, `with_idle_tracker`, `with_host_prompts`, `with_host_keypair`,
`with_ssh_agent_key_adder`, `with_host_user_files`, `with_host_tooling`, `with_host_registry`,
`with_eligible_daemon_source`, `with_host_stats`, `with_worktree_size_calculator`,
`with_host_stats_intervals`, `with_workspace_sandbox_provisioner`, `with_room_roster`,
`with_room_poll_interval`, `with_roster_keepalive_interval`).

**It implements 7 traits from 4 crates** — `ConnectionServiceTrait` (:11480),
`session_room::RemoteSnapshotSource` (:3475), `session_room::SessionTerminalBridge` (:4626),
`StackParentHost` (:1830), `tddy_core::toolcall::ChildSpawnHandler` (:8280),
`tddy_core::toolcall::ConversationSpawnHandler` (:8889),
`tddy_sandbox_runner::HostRpcHandler` (:11354). **It defines only 2** — `SeededAgentClones` (:1575)
and `StackParentHost` (:1660).

Note the direction: `impl crate::session_room::{RemoteSnapshotSource, SessionTerminalBridge} for
ConnectionServiceImpl` means **the god file depends on `session_room`'s traits, not vice versa** —
which is the direction extraction wants.

Symbols other subsystems pull out of it, with locations:

```
129:   pub(crate) async fn spawn_blocking_with_timeout<T>
194:   pub(crate) async fn push_new_branch_to_origin_if_requested
1195:  pub(crate) fn now_unix_ms() -> u64
1212:  pub struct AgentActivityHub      (StdMutex<HashMap<String, broadcast::Sender<…>>> + pending)
1520:  pub struct SeedCodebase
3189:  pub fn local_daemon_hook_url(&DaemonConfig) -> String
3256:  pub fn effective_spawn_branch<'a>
3354:  pub fn spawned_branch_of_session
3667:  pub(crate) fn started_roster_rev
18060: pub const HOST_DOCUMENT_FRAME_BYTES: usize = 48 * 1024;
21943: pub enum WorktreeSource
21952: pub fn session_worktree_source
```

#### `src/auth.rs` (1058 = 435 prod / 623 inline test)

Its **only** dependency on the god file is one type alias — `auth.rs:25: use
crate::connection_service::SessionUserResolver;`

```rust
pub struct AuthBuildResult {
    pub entries: Vec<ServiceEntry>,
    pub user_resolver: Option<SessionUserResolver>,
    /// `Some` when `auth_storage` is configured. Shared with `ConnectionServiceImpl`…
    pub github_token_store: Option<Arc<dyn GitHubTokenStore>>,
}
```

The signing secret is `config.livekit.api_secret` via `tddy_github::SessionTokenSigner` — **the same
secret signs LiveKit room JWTs and session tokens.**

#### `src/session_room.rs` (2815) — cluster-ambiguous, settled

It is the **LiveKit per-worktree room** (`tddy_livekit::{BroadcastPublisher, JoinedParticipant,
RoomMetadataClient, RpcService, TokenGenerator}`). Defines 4 traits — `SessionTerminalBridge`
(:1329), `WorktreeSource` (:1986), `SessionTokenMinter` (:2024), `RemoteSnapshotSource` (:2033) —
two of which `ConnectionServiceImpl` implements. **Inversion of control is already in place here.**

#### `src/model_registry/` — 13 files, 3,035 non-blank, **0 inline test LoC**

`mod.rs` (33) declares `acp_service, assistant_def, error, labels, ollama, openai_compatible,
provider_client, provider_http, service, store, tool_dispatcher, workspace`. **Zero outbound
`crate::` edges beyond its own directory.** The only already-directory-shaped subsystem, and the
cleanest extraction candidate in the crate.

#### the contract a subsystem crate must satisfy — `tddy-rpc/src/bridge.rs:72`

```rust
pub struct ServiceEntry { pub name: &'static str, pub service: Arc<dyn RpcService> }
pub struct MultiRpcService { entries: Vec<ServiceEntry> }
```

That is **all** the wiring layer needs from a subsystem: one `build_*_entry(…) -> ServiceEntry`.

#### dead / misplaced code found in passing

- **`vnc_service.rs` (204) + `vnc_vault.rs` (359) are never registered.** `runtime.rs` registers
  `screen_sharing.ScreenSharingService` but no `vnc.VncService`. With their tests
  (`vnc_service_acceptance.rs` 268, `vnc_vault_acceptance.rs` 177) that is **1,008 LoC of an
  unreachable RPC service.**
- **`tool_catalog_sync.rs` (31) is a test file living in `src/`** — its entire body is one
  `#[cfg(test)] mod tests` with a single test.
- **`codex_oauth_relay.rs` (275) is used only by `tddy-integration-tests`** — no daemon module
  references it and `runtime.rs` does not wire it.
- `claude_cli_session.rs` (5), `pty_registry.rs` (5), `relay_idle.rs` (6) are re-export shims.

### the 15 subsystems

Verified to partition all 106 files with no overlap and no omission (prod sums to exactly 63,353,
total to exactly 82,599).

| Subsystem | Files | prod LoC | inline-test | total | dedicated test files / LoC |
|---|---:|---:|---:|---:|---|
| **CONNECTION-CORE** | 4 | **18,765** | 6,002 | 24,767 | (80 of 166 files reach it) |
| **TELEGRAM** | 10 | 6,835 | 1,201 | 8,036 | 14 / 4,529 |
| **LIVEKIT / ROOMS / SCREENSHARE** | 8 | 7,912 | 1,443 | 9,355 | 18 / 6,898 + 4 / 1,069 |
| **SESSIONS** | 19 | 7,239 | 3,576 | 10,815 | 45 / 17,986 (6 groups) |
| **MODEL-REGISTRY** | 13 | 3,035 | **0** | 3,035 | 8 / 4,618 |
| **GIT / WORKTREES / PROJECTS** | 8 | 2,901 | 903 | 3,804 | 23 / 6,807 + 3 / 1,590 |
| **SPAWN / SUPERVISOR** | 4 | 2,769 | 467 | 3,236 | 5 / 728 |
| **CORE-WIRING** | 6 | 2,715 | 984 | 3,699 | 10 / 2,789 |
| **HOSTS** | 9 | 2,417 | 1,250 | 3,667 | 2 / 445 |
| **AUTH / SECRETS** | 11 | 2,277 | 1,862 | 4,139 | **2 / 194** |
| **CONTEXT / DOCS** | 6 | 2,197 | 675 | 2,872 | 10 / 4,026 |
| **SANDBOX** | 6 | 2,171 | **62** | 2,233 | **16 / 5,494** |
| **MISC RPC SERVICES** | 10 | 1,448 | 554 | 2,002 | (in MISC-WIRING / RELAY) |
| **PTY / TERMINAL** | 3 | 339 | 106 | 445 | — |
| **TOOL-CALLS** | 2 | 333 | 161 | 494 | — |
| **TOTAL** | **106** | **63,353** | **19,246** | **82,599** | **167 / 58,174** |

Exact file lists (non-blank LoC each):

- **CORE-WIRING** — `config.rs` 2204 · `runtime.rs` 1089 · `main.rs` 168 · `lib.rs` 109 · `server.rs` 98 · `startup.rs` 31
- **CONNECTION-CORE** — `connection_service.rs` 23099 · `connection_tonic_adapter.rs` 1437 · `local_socket_server.rs` 178 · `test_util.rs` 53
- **TELEGRAM** — `telegram_session_control.rs` 4158 · `telegram_notifier.rs` 1846 · `telegram_bot.rs` 737 · `telegram_github_link.rs` 294 · `telegram_tracked_session.rs` 287 · `active_elicitation.rs` 224 · `elicitation.rs` 197 · `telegram_session_subscriber.rs` 119 · `telegram_multi_select_shortcuts.rs` 87 · `presenter_intent_client.rs` 87
- **LIVEKIT/ROOMS/SCREENSHARE** — `session_room.rs` 2815 · `livekit_peer_discovery.rs` 2215 · `screen_sharing_service.rs` 1953 · `common_room_supervisor.rs` 768 · `livekit_rooms_stream.rs` 695 · `vnc_vault.rs` 359 *(unwired)* · `screen_sharing_vault.rs` 346 · `vnc_service.rs` 204 *(unwired)*
- **SESSIONS** — `session_list_enrichment.rs` 1740 · `cli_session_manager.rs` 1538 · `session_agent_clone.rs` 1173 · `split_session.rs` 1142 · `session_agent_status.rs` 685 · `session_deletion.rs` 656 · `session_agent_roster.rs` 605 · `session_attachments.rs` 570 · `cursor_cli_spawn.rs` 494 · `session_notifications.rs` 433 · `session_attachment_staging.rs` 370 · `workspace_session.rs` 324 · `session_agent_inference.rs` 285 · `session_admission_service.rs` 205 · `session_notification_subscribers.rs` 194 · `session_uploads.rs` 137 · `session_reader.rs` 136 · `session_file_upload.rs` 123 · `claude_cli_session.rs` 5
- **MODEL-REGISTRY** — `store.rs` 1075 · `acp_service.rs` 617 · `service.rs` 380 · `ollama.rs` 199 · `openai_compatible.rs` 166 · `error.rs` 117 · `assistant_def.rs` 108 · `provider_http.rs` 93 · `tool_dispatcher.rs` 81 · `labels.rs` 69 · `workspace.rs` 59 · `provider_client.rs` 38 · `mod.rs` 33
- **GIT/WORKTREES** — `worktrees.rs` 967 · `remote_git_service.rs` 787 · `project_storage.rs` 599 · `worktree_files.rs` 573 · `project_provision.rs` 387 · `branch_intent.rs` 322 · `base_sync_cache.rs` 119 · `branch_owner.rs` 50
- **SPAWN/SUPERVISOR** — `spawner.rs` 2296 · `spawn_worker.rs` 527 · `supervisor_spawn.rs` 342 · `supervisor_client.rs` 71
- **HOSTS** — `host_tooling.rs` 989 · `host_registry.rs` 974 · `host_prompts.rs` 394 · `host_stats.rs` 307 · `remote_desktop_probe.rs` 289 · `host_desktop_targets.rs` 251 · `host_session_service.rs` 169 · `multi_host.rs` 149 · `host_prompt_stream.rs` 145
- **AUTH/SECRETS** — `auth.rs` 1058 · `host_private_key.rs` 880 · `ssh_agent.rs` 723 · `oauth_loopback_tunnel.rs` 345 · `github_token_store.rs` 318 · `codex_oauth_relay.rs` 275 · `host_keypair.rs` 263 · `ssh_agent_add.rs` 128 · `codex_oauth_participant_metadata.rs` 92 · `github_pr_credentials.rs` 44 · `token_provider.rs` 13
- **CONTEXT/DOCS** — `host_documents.rs` 595 · `context_sync.rs` 595 · `session_context_docs.rs` 558 · `context_files.rs` 483 · `stack_doc_attachments.rs` 441 · `session_workflow_files.rs` 200
- **SANDBOX** — `sandbox_session.rs` 1017 · `workspace_tool_sandbox.rs` 477 · `sandbox_action.rs` 407 · `sandbox_plan_builder.rs` 263 · `sandbox_runtime.rs` 38 · `tool_catalog_sync.rs` 31 *(test-only file)*
- **MISC RPC SERVICES** — `daemon_settings.rs` 550 · `action_service.rs` 329 · `task_service.rs` 322 · `daemon_config_service.rs` 204 · `user_sessions_path.rs` 189 · `bsp_service.rs` 177 · `agent_list_mapping.rs` 94 · `semantic_index.rs` 68 · `tddy_user_config.rs` 63 · `relay_idle.rs` 6
- **PTY/TERMINAL** — `pty_runtime.rs` 366 · `terminal_session_adapter.rs` 74 · `pty_registry.rs` 5
- **TOOL-CALLS** — `tool_call_log.rs` 276 · `session_toolcall.rs` 218

### verdicts on the expected clusters

| Expected cluster | Verdict |
|---|---|
| telegram | **Confirmed, and larger than the name suggests** — `active_elicitation`, `elicitation`, `presenter_intent_client` belong with it. 8,036 LoC |
| hosts | **Confirmed** — but `host_documents` is CONTEXT/DOCS, `host_keypair`/`host_private_key` are AUTH/SECRETS, `host_session_service` is a stdio-pipe RPC only `connection_service` uses |
| sandbox + spawn + supervisor | **Refuted as one cluster — these are two.** SANDBOX (confinement, 6/2,233) and SPAWN/SUPERVISOR (fork+setuid privilege, 4/3,236) share no `crate::` edge; `sandbox_*` never touches `spawner` |
| sessions | **Confirmed but too coarse for one crate** — 19 files / 10,815 LoC over ≥5 sub-clusters: agent-roster, notifications, attachments/uploads, CLI process lifecycle, placement |
| livekit/screenshare | **Confirmed**, but `session_room.rs` (2,815) is the heaviest member and `screen_sharing_service` (1,953) has **zero** edge to `connection_service`. `vnc_*` is **dead** |
| git/worktrees | **Confirmed.** `worktrees.rs`, `worktree_files.rs`, `project_storage.rs`, `base_sync_cache.rs`, `branch_intent.rs` have **zero** outbound `crate::` edges |
| auth/secrets | **Confirmed.** Coupling to the god file is **one type alias** |
| `model_registry/` | **Confirmed and cleanest** — 0 outbound edges, 0 inline tests |
| context/docs | **Confirmed** |
| pty/terminal | **Refuted — already extracted.** 445 LoC, of which `pty_registry.rs` is a 5-line re-export |
| tool-calls | **Refuted as one cluster.** `tool_call_log` + `session_toolcall` (494, zero outbound) are cohesive; `elicitation*` are Telegram; `tool_catalog_sync` is a test file; `tool_engine` is already external |

### the immovable core

**Tier 0 — must stay, and is already small: 2,715 prod LoC** across `main.rs`, `lib.rs`, `server.rs`,
`startup.rs`, `runtime.rs`, `config.rs`.

**Tier 1 — the real knot: `connection_service.rs`, 17,151 prod LoC** (27% of the crate's production
code). Why it cannot move as a unit: it *is* the `connection.ConnectionService` RPC surface (~5,700
prod LoC of handler bodies); 60 fields aggregating every registry in the daemon; the sharing
mechanism is `Arc` + shallow `Clone` + a `Weak` self-handle with three `self_arc()` call sites; it
implements 7 traits from 4 crates; 5,948 LoC of inline tests interleaved in 29 mods; and 80 of 166
integration tests reach it, most via `test_util::test_service`.

**Tier 2 — near-core**: `auth.rs` (the identity boundary, one shared signing secret),
`connection_tonic_adapter.rs` + `local_socket_server.rs` (a second transport for the same impl,
generic over `T` so they depend only on the trait), and `test_util.rs` (the fixture 80 tests build on).

### what makes the move tractable

1. **30 `pub trait` ports already exist** — `HostRegistry`, `HostToolingProbe`, `HostPromptRegistry`,
   `HostKeypair`, `HostDesktopTargetStore`, `HostUserFiles`, `HostStats`, `SshAgentKeyAdder`,
   `AgentSocketResolver`, `SshAgentProbe`, `RemoteDesktopProbe`, `RoomRoster`, `EligibleDaemonSource`,
   `CommonRoomConnector`, `ConnectedCommonRoom`, `CommonRoomSupervisor`,
   `SessionNotificationSubscriber`, `TelegramSender`, `ContextSource`, `WorkspaceSandbox`,
   `WorkspaceSandboxProvisioner`, `SessionTerminalBridge`, `WorktreeSource`, `SessionTokenMinter`,
   `RemoteSnapshotSource`, `SeededAgentClones`, `StackParentHost`, `ProviderClient`,
   `ProviderClientFactory`. **Every one of the 21 `with_*` builders injects behind a trait.**
2. **The extraction pattern is established three times**, with 5–7 line `pub use` shims.
3. **`ServiceEntry` is the only contract.**
4. **Six subsystems have essentially no inbound coupling and can go first**, in order of ease:
   `model_registry/` (3,035, 0 inline tests, 0 outbound edges, 8 dedicated test files) → SANDBOX
   (2,171 prod, 62 inline tests, 5,494 integration LoC — after the shared symbols are lifted) →
   `screen_sharing_*` (2,299, 0 inline tests, zero `connection_service` edge) → `vnc_*` (563 —
   **or delete**) → MISC RPC services `action_service`/`task_service`/`bsp_service` (828 prod, all
   leaf, `runtime`-only in-edge) → TOOL-CALLS (494, zero outbound).

### the five extraction blockers inside `connection_service.rs`

These shared symbols must leave the god file **before** any subsystem crate can compile:

| Symbol | Location | Reached by |
|---|---|---|
| `AgentActivityHub` | `:1212` | SANDBOX (`sandbox_session.rs` ×5), SESSIONS (`session_agent_inference.rs:36`) |
| `now_unix_ms()` | `:1195` | SANDBOX (×2), TELEGRAM (`telegram_session_subscriber.rs:122`) |
| `HOST_DOCUMENT_FRAME_BYTES` | `:18060` | CONTEXT (`context_files.rs:41`) |
| `SessionUserResolver` / `SessionsBaseResolver` | `:236` / `:239` | AUTH (`auth.rs:25`) + 5 more subsystems |
| the 9-symbol spawn preamble | `:129,194,3256,3354,3667,1520,1575,1660,3189` | SESSIONS (`cursor_cli_spawn.rs`, 7 sites) — **the worst single edge in the crate** |

### the nine cycles that must be broken before any crate split

Rust crates cannot be mutually dependent:

| Cycle | Cut |
|---|---|
| `config → session_room` vs `session_room → config` (`config.rs:85`) | **inline `DEFAULT_GIT_TIMEOUT` into `config.rs`** — otherwise the wiring layer depends on LIVEKIT and vice versa |
| `auth → connection_service → auth` (`auth.rs:25`) | move the two resolver aliases to a shared types crate |
| `host_tooling ⇄ ssh_agent` | **splits HOSTS from AUTH** |
| `livekit_peer_discovery ⇄ multi_host` | `multi_host` is really LiveKit routing, not HOSTS |
| `common_room_supervisor → daemon_config_service → livekit_peer_discovery → common_room_supervisor` | spans LIVEKIT and MISC |
| `context_files → worktree_files → context_files` | internal to the CONTEXT/GIT boundary |
| `session_agent_status ⇄ session_agent_inference` | resolved by lifting `AgentActivityHub` |
| `host_tooling ⇄ remote_desktop_probe` | internal to HOSTS, benign |
| `telegram_notifier ⇄ telegram_session_control ⇄ telegram_bot` | internal to TELEGRAM, benign |

### proto packages per subsystem

`tddy_service::proto::<package>`, generated at build time via
`include!(concat!(env!("OUT_DIR"), "/<pkg>.rs"))`; `src/gen/` is empty on disk.

| Proto package | Daemon files |
|---|---|
| **`connection`** | **20 files** — `connection_service`, `connection_tonic_adapter`, `context_files`, `cursor_cli_spawn`, `host_documents`, `host_prompt_stream`, `livekit_peer_discovery`, `livekit_rooms_stream`, `local_socket_server`, `multi_host`, `sandbox_session`, `session_agent_clone`, `session_agent_inference`, `session_agent_roster`, `session_agent_status`, `session_list_enrichment`, `split_session`, `stack_doc_attachments`, `workspace_session`, `workspace_tool_sandbox`. **`connection.proto` is as much a god-schema as the impl is a god-module** |
| `models` | `agent_list_mapping`, all of `model_registry/` |
| `acp` | `connection_service`, `model_registry/acp_service`, `session_agent_inference` |
| `auth`, `token` | `auth` |
| `daemon_config` | `daemon_config_service`, `daemon_settings`, `runtime` |
| `screen_sharing` | `screen_sharing_service`, `screen_sharing_vault` |
| `session_admission` | `session_admission_service`, `session_agent_clone` |
| `worktree_activity` | `session_room`, `session_agent_clone` |
| `remote_git` | `remote_git_service` |
| `terminal` | `cli_session_manager` |
| `tasks` | `task_service` |
| `actions` | `action_service` |
| `bsp` | `bsp_service` |
| `vnc` | `vnc_service` *(unwired)* |
| `sandbox` | `workspace_tool_sandbox` |
| `loopback_tunnel` | `oauth_loopback_tunnel` |

### dependencies attributable to exactly one subsystem (they leave with it)

`sqlx` → MODEL-REGISTRY · `agent-client-protocol` + `tddy-acp` → MODEL-REGISTRY · `teloxide` →
TELEGRAM · `ssh-agent-lib`, `ssh-key`, `rsa`, `argon2`, `chacha20poly1305`, `hmac`, `sha2`, `subtle`
→ AUTH/SECRETS · `portable-pty` → PTY · `sysinfo` → HOSTS · the six `tddy-sandbox*` crates → SANDBOX
· `tddy-supervisor` → SPAWN · `tddy-session-sync` → SESSIONS · `tddy-screenshare` → SCREENSHARE ·
`tddy-lsp`, `tddy-lsp-executor`, `tddy-connectrpc` → CORE-WIRING.

`tddy-daemon` has **28 workspace path dependencies** plus a `local-model` feature. **No subsystem
depends on `tddy-coder` except CORE-WIRING, SPAWN (`spawner.rs`) and `model_registry/store.rs`** — so
the web bundle is genuinely a wiring concern.

### external API constraints

Only 3 crates link `tddy-daemon` as a library:

- `tddy-desktop/src-tauri` → `tddy_daemon::{config, runtime, supervisor_client, spawn_worker,
  cli_session_manager}` — **exactly the wiring layer plus two subsystems**, confirming
  `RuntimeHost::Embedded` is the intended embedding contract.
- `tddy-sandbox-app` → `tddy_daemon::{sandbox_session, claude_cli_session, tool_engine}` — **this
  dependency reverses** if SANDBOX becomes a crate.
- `tddy-integration-tests` → `tddy_daemon::codex_oauth_relay` only.

### test organisation

166 integration test files + `tests/common/mod.rs`, 58,174 non-blank LoC; a further 19,246 test LoC
are inline in `src/`. **Organised per subsystem by naming, but not by dependency**: 80 of 166 files
import `tddy_daemon::connection_service`, and the dominant idiom is
`tddy_daemon::{claude_cli_session, config, connection_service, test_util}` — build a
`ConnectionServiceImpl` via `test_util`, then drive it through the `connection.ConnectionService` RPC.

**Roughly 60 files can move as-is today** (they import only their own subsystem's modules): all of
MODEL-REGISTRY bar 2, 7 SANDBOX files, all 4 SCREEN-VNC, 10 LIVEKIT, 11 of 14 TELEGRAM, 9 GIT, 6
CONTEXT, 4 SPAWN, 3 NOTIFICATIONS unit tests, and 8 MISC.

Worst test-locality mismatch: **AUTH/SECRETS** — 1,862 LoC of inline tests but only 2 dedicated test
files (194 LoC), plus `ssh_agent_block_handler_tests` and `host_add_key_handler_tests` living inside
`connection_service.rs`. Best: **SANDBOX** — 62 inline vs 5,494 integration.

`tests/common/mod.rs` (41 LoC, `a_capture_showing` + `PTY_STUB_OUTPUT`, used by 6 files) would need
duplicating or promoting into `tddy-testing-commons`.

### Findings

1. **The wiring layer already exists and is already thin — 2,715 prod LoC in 6 files.** The brief's
   target state is therefore reachable by *removing* things, not by writing a new wiring layer.
   `runtime.rs`'s doc contract ("nothing that listens, dials or runs forever is started there") and
   `ServiceEntry { name, service }` are the seams to build every node against.
2. **Coupling to the god file is far weaker than its size suggests.** Only 8 files outside it hold a
   real code reference, and 4 of those are a single symbol each. Every other apparent reference is a
   doc comment.
3. **Five shared symbols and nine cycles are the whole gate.** Lift `AgentActivityHub`,
   `now_unix_ms`, `HOST_DOCUMENT_FRAME_BYTES`, the two resolver aliases and the 9-symbol spawn
   preamble; cut `config.rs:85` and `auth.rs:25`; then most subsystems are free-standing.
   **This is the natural root node of the stack.**
4. **30 trait ports and 21 trait-injecting builders mean the abstractions are already in place** —
   the daemon depends on abstractions for most collaborators, so extraction is mostly relocation.
5. **`connection.proto` is a god-schema referenced by 20 files**, which is why splitting the proto is
   the load-bearing change and not a cosmetic one.
6. **`tddy-desktop` consumes exactly `{config, runtime, supervisor_client, spawn_worker,
   cli_session_manager}`** — the wiring layer. It is also outside the CI gate, so it is the one
   consumer a node can break invisibly.
7. **1,008 LoC of `vnc_*` is unreachable dead code**, `tool_catalog_sync.rs` is a test file in
   `src/`, and `codex_oauth_relay.rs` is reached only from `tddy-integration-tests`. Deleting or
   relocating these is free scope for the nodes that touch those areas.
8. **`tddy-sandbox-app`'s dependency reverses** when SANDBOX becomes a crate — a concrete, checkable
   outcome for that node.

## Exploration 6: the proto / codegen / consumer pipeline — 2026-09-09

**Agent**: Explore subagent
**Scope**: every `.proto` in the repo; the complete `ConnectionService` method inventory grouped by
family; how Rust and TypeScript are generated; the role of each RPC package; the trait surface; the
full consumer map; `tddy-web`'s dependency resolution and the local-registry scripts; recorded
wire-compatibility policy.

### Sequence

1. `find . -name '*.proto'` — **output polluted by 11 agent worktrees under `.claude/worktrees/`**; re-ran excluding `./.claude/*`, `./.worktrees/*`, `./tmp/*`.
2. `grep -n '^\s*service '` per proto; then an `awk` service-block scanner for rpc count + line range.
3. Read `connection.proto` lines 1–370 in full — the `ConnectionService` block **with its doc comments**, which are the grouping evidence.
4. `grep -rni 'telegram'` in `proto/`; `grep -n 'ScreenShar\|LiveKit\|Telegram'` in connection.proto — test the parent's guessed families.
5. `find ./packages -name 'build.rs'`; `find . -name 'buf*.yaml'` — codegen entry points.
6. Read `packages/tddy-service/build.rs` (418 lines) in full.
7. `cat` every `buf.gen.yaml` / `buf.yaml`.
8. `ls -la packages/tddy-web/src/gen`; `git check-ignore`; `git ls-files` — is generated TS committed?
9. `cat packages/tddy-web/package.json`, root `package.json`, `bunfig.toml`; search for `.npmrc`.
10. `cat scripts/local-bun-install.sh`, `scripts/resolve-local-lock.ts`.
11. Read `packages/tddy-service/src/lib.rs` (433), `packages/tddy-codegen/src/generator.rs` (902), `packages/tddy-daemon/src/connection_tonic_adapter.rs` head (1560 total).
12. `grep -rln 'ConnectionService'` over all `src`/`tests` — consumer inventory.
13. camelCase-alternation grep of all 90 method names over `packages/tddy-web/src` — per-method call sites.
14. `grep -rn '"connection.ConnectionService"'` repo-wide — the service-name **string** as a routing key.
15. Per-package `grep -rl` for `proto::connection|connection_pb|tonic_connection` — prove the negatives.
16. `ls -la */node_modules | grep tddy`; `python3` parse of `bun.lock` workspaces — how tddy-web resolves today.
17. `grep 'reserved '` all protos; `grep -i 'breaking change\|wire compat\|backward compat'` over docs.
18. `grep 'TerminalSessionService\|terminal_session\.'` repo-wide excluding its own package.
19. `.github/workflows/ci.yml` grep for `bun install` / `buf`.

### Grep / glob

| Pattern | Scope | Notable |
|---|---|---|
| `-name '*.proto'` | repo | **51 protos** outside worktrees/tmp (plus ~50 duplicates inside `.claude/worktrees/agent-*` — always exclude) |
| `^\s*service ` | all protos | 30 service blocks in 25 files; all 16 `tddy-workflow-recipes` protos and both `tddy-build` protos declare **none** |
| awk rpc counter | all protos | **`connection.proto:7-367 ConnectionService rpcs=90`**; next largest is `ModelRegistryService` at **12** |
| `returns (stream` in lines 7-367 | connection.proto | **21** server-streaming |
| `rpc [A-Za-z]*(stream ` | connection.proto | **1** bidi (`StreamSessionTerminalIO`) |
| `^message \|^enum ` | connection.proto | **223 messages, 15 enums** |
| `reserved ` | all protos | only two in connection.proto: `:1031 reserved 19, 20, 21, 22, 23;`, `:2176 reserved 5;` |
| `telegram` (-i) | `tddy-service/proto` | only two comment mentions. **No Telegram rpc in `ConnectionService`** — it is `tddy/v1/observer.proto` + `tddy/v1/presenter_intent.proto` |
| `"connection.ConnectionService"` | repo | **139** tddy-web, 43 tddy-daemon, 28 tddy-tools, 21 tddy-coder, 16 tddy-rust-typescript-tests, 10 tddy-rpc, 7 tddy-sandbox-runner, 6 tddy-livekit-web, 6 tddy-discovery, 3 tddy-session-sync, 3 tddy-sandbox-app |
| `gen/connection_pb` | `packages/tddy-web` | **244 hits — 116 in 112 `src/` files, 130 in 126 `cypress/` files** |
| `\.(<90 camelCase methods>)\(` | tddy-web `src/` | **86 call sites across 45 files** |
| `TerminalSessionService\|terminal_session\.` | repo minus its own package | **0 hits — it is served nowhere** |
| `impl .*ConnectionService for` | Rust | only `connection_tonic_adapter.rs` (tonic flavour) and `connection_service.rs` (tddy-rpc flavour); **tddy-coder implements `RpcService` instead** |
| `buf\|generate\|_pb.ts` | `.github/workflows/*.yml` | **0 hits — no codegen-drift gate in CI** |

### Inspected files

#### `packages/tddy-service/proto/connection.proto` — 2,953 lines

`package connection;` — **no `v1`**, unlike `tddy/v1/*`, `tddy/acp/v1/*`, `tddy/build/v1/*`.
`service ConnectionService` spans **lines 7–367** with **90 rpc methods** (69 unary, 21
server-streaming, 1 bidi), followed by **223 messages and 15 enums**.

#### `packages/tddy-service/build.rs` (418 lines) — 21 codegen passes; the two `connection.proto` ones

```rust
// Connection service (daemon session/tool management)
prost_build::Config::new()
    .out_dir(std::env::var("OUT_DIR")?)
    .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
        generate_rpc_server: true, generate_tonic_adapter: false,
        rpc_crate_path: "tddy_rpc".to_string(),
    }))
    .compile_protos(&["proto/connection.proto"], &["proto"])?;

// Connection service (tonic gRPC server/client, reusing the canonical prost message types).
// `.extern_path(".connection", ...)` remaps every `.connection.*` message to the structs
// generated by the tddy-rpc pass above, so this pass emits only service code … no duplicate
// message structs.
tonic_build::configure().build_server(true).build_client(true)
    .out_dir(&tonic_connection_dir)
    .extern_path(".connection", "crate::proto::connection")
    .compile_protos(&["proto/connection.proto"], &["proto"])?;
```

**The sandbox pass `extern_path`s three `connection.*` messages** (lines 288–299):

```rust
.extern_path(".connection.SessionTerminalOutput", "crate::proto::connection::SessionTerminalOutput")
.extern_path(".connection.ExecuteToolRequest",   "crate::proto::connection::ExecuteToolRequest")
.extern_path(".connection.ExecuteToolResponse",  "crate::proto::connection::ExecuteToolResponse")
```

→ **splitting `connection.proto` breaks `sandbox.proto`'s generated code unless these three extern
paths are re-pointed.** A descriptor-set pass over **19 protos** feeds gRPC reflection;
`supervisor.proto` is deliberately excluded so "the one process that runs as uid 0 does not link
this crate's dependency tree".

#### `packages/tddy-codegen/src/generator.rs` (902) — what the trait actually is

`TddyServiceGenerator` emits per service: an `#[async_trait] pub trait <Name>` with tonic-mirrored
signatures over `tddy_rpc::{Request, Response, Status, Streaming}`, an associated
`type XStream: Stream<…>` per server-streaming method, and a
`pub struct <Name>Server<T> { inner: Arc<T> }` carrying
`pub const NAME: &'static str = "<package>.<Name>"` plus an `impl RpcService` that **dispatches on
the method-name string**. `generate_tonic_adapter` is a **documented stub** (`generator.rs:872-902`:
*"TODO: Full impl of tonic server trait requires consumer to compile proto with tonic-build."*) —
which is exactly why `connection_tonic_adapter.rs` is 1,560 hand-written lines.

**"111 members" checks out exactly: 90 methods + 21 associated `…Stream` types.**

#### `packages/tddy-daemon/src/connection_tonic_adapter.rs` — module doc, verbatim

```rust
//! Adapter that serves an existing tddy-rpc `ConnectionService` implementation … over native
//! tonic gRPC. … Both flavors reference the identical canonical prost message types (via
//! `extern_path`…), so no message re-encode/decode is needed — only the transport wrappers and
//! the error `Status`. The tddy-rpc `tonic` feature provides `From` impls … but it pins tonic
//! 0.11 while the generated tonic service targets tonic 0.12 …
//! Each method is spelled out as a literal `async fn` (rather than generated by a declarative
//! macro) because `#[tonic::async_trait]` rewrites method signatures and cannot see through a
//! macro invocation.
```

One method is **not** delegated: `MintLocalToken` is handled in the adapter itself, reading
`UdsConnectInfo` (SO_PEERCRED). `pub type UidToUsername = Arc<dyn Fn(u32) -> Option<String> + …>;`

#### `packages/tddy-coder/src/session_participant/mod.rs` — the **second server**, and the lockstep hazard

Registers the same service name (`:107-110`, `:155-161`) but **does not implement the generated
trait**:

```rust
/// `RpcService` adapter that dispatches the session-scoped `ConnectionService` methods to a
/// [`SessionConnectionService`]. Methods not served by the session participant (delete/signal,
/// project listing, session start/resume, terminal streaming, …) return `Unimplemented` — the web
/// routes them to the daemon participant instead.
struct SessionConnectionServiceRpc { svc: Arc<SessionConnectionService> }
```

`impl RpcService` is a `match method { "ListExecTools" => …, "ExecuteTool" => …, … }` string
dispatch with manual prost decode/encode. It names 14 methods across families K, L, M and N.

#### `packages/tddy-terminal-rpc/proto/terminal_session.proto` (235) — the precedent, and its warning

Declares `TerminalSessionService` with **9 rpcs that duplicate 9 `ConnectionService` methods**
exactly (family K below). Its header says it *"Consolidates the terminal streaming bridge logic that
was previously duplicated between the two…"*, and `build.rs` uses the same two-pass pattern
*"mirroring the pattern in `tddy-service/build.rs`"*.

**But `TerminalSessionService` is served nowhere** — zero hits repo-wide outside its own package.
What is actually reused is the *bridge* (`serve_stream_terminal_output_with`,
`serve_get_terminal_history_with`, `serve_stream_session_terminal_io_with`, and the
`TerminalSession`/`TerminalSessionStore` traits), called from `connection_service.rs:14506` and
`tddy-coder/src/session_participant/mod.rs:374,441`, whose call sites **convert**
`connection.SessionTerminalInput` → `terminal_session.SessionTerminalInput` **by hand**
(`connection_service.rs:439-457`).

**This is the cautionary precedent for the whole stack: extracting a proto without moving the served
coordinate leaves dead code plus a hand-written converter.**

### the 90-method inventory, grouped by family

Fully-qualified name `connection.ConnectionService`, block lines 7–367.

| # | Family | Methods | Count | Streaming |
|---|---|---|---:|---|
| **A** | Tools & agents catalogue | `ListTools`, `ListAgents`, `ListAgentModels`, `ListSubagents` | 4 | — |
| **B** | Session agent roster + agent conversations | `AttachSessionAgent`, `DetachSessionAgent`, `ListSessionAgents`, `StreamSessionAgents`▸, `OpenAgentConversation`, `PromptAgentConversation`▸, `CancelAgentConversation`, `ReportAgentCloneState`, `ReportAgentConversationState` | 9 | 2 |
| **C** | Sessions lifecycle | `ListSessions`, `StartSession`, `ConnectSession`, `ResumeSession`, `SignalSession`, `DeleteSession`, `StreamStartSession`▸, `GetWorktreeSnapshot` | 8 | 1 |
| **D** | Projects & branches | `ListProjects`, `CreateProject`, `AddProjectToHost`, `ListProjectBranches`, `SetProjectDefaultBranch` | 5 | — |
| **E** | Hosts / registry / tooling / prompts & keys | `ListEligibleDaemons`, `ListKnownHosts`, `GetHostTooling`, `StreamHostPrompts`▸, `AnswerHostPrompt`, `AddHostKey`, `ListHostKeyCandidates` | 7 | 1 |
| **F** | Host telemetry | `StreamHostStats`▸ | 1 | 1 |
| **G** | Worktrees — listing, disk usage, lifecycle | `ListWorktreesForProject`, `RemoveWorktree`, `StreamWorktreeStats`▸, `CalculateWorktreeSize`, `CleanWorktree`, `RestoreSessionWorktree` | 6 | 1 |
| **H** | Worktree file browsing / reading | `ListWorktreeDirectory`, `ReadWorktreeFile`, `StreamReadWorktreeFile`▸ | 3 | 1 |
| **I** | Session workflow files | `ListSessionWorkflowFiles`, `ReadSessionWorkflowFile` | 2 | — |
| **J** | Agent context sync | `StreamContextManifest`▸, `StreamReadContextFile`▸, `StreamReadContextFileBatch`▸ | 3 | 3 |
| **K** | Terminals — PTY I/O, history, control mutex | `StreamSessionTerminalIO`◆, `StreamTerminalOutput`▸, `SendTerminalInput`, `GetTerminalHistory`▸, `StartTerminalSession`, `StopTerminalSession`, `ListTerminalSessions`, `ClaimTerminalControl`, `WatchTerminalControl`▸ | 9 | 4+bidi |
| **L** | Tool execution | `ExecuteTool`, `StreamExecuteTool`▸, `ListExecTools`, `ListSessionToolCalls` | 4 | 1 |
| **M** | Agent activity, session status, notifications | `ReportSessionStatus`, `StreamSessionActivity`▸, `ReportAgentActivity`, `StreamSessionNotifications`▸, `StreamAgentActivityDelta`▸ | 5 | 3 |
| **N** | ACP transcript replay | `StreamAcpReplay`▸, `GetAcpToolCallDetail`, `GetAcpReplayPage` | 3 | 1 |
| **O** | Demo VM lifecycle | `StartDemoVm`, `StopDemoVm`, `GetDemoVmStatus` | 3 | — |
| **P** | PR-stack / git branch orchestration | `AddPlannedPr`, `GetPrStatus`, `RepointPlannedPr`, `ReorderPlannedPr`, `PullBaseIntoBranch`, `QueryBranch`, `ResolveStackBase`, `LinkStackNode` | 8 | — |
| **Q** | Auth / local peer-trust | `MintLocalToken` — **handled in the tonic adapter, UDS transport only** | 1 | — |
| **R** | Session file uploads | `UploadSessionFileChunk`, `ListSessionUploads`, `DeleteSessionUpload` | 3 | — |
| **S** | Staged attachments + host documents | `UploadStagedAttachmentChunk`, `ListStagedAttachments`, `DeleteStagedAttachment`, `ReadHostDocument`, `StreamReadHostDocument`▸ | 5 | 1 |
| **T** | LiveKit rooms observability | `StreamLiveKitRooms`▸ | 1 | 1 |

▸ server-streaming · ◆ bidi

**Families that are NOT in `ConnectionService`** (they already have their own protos, so those
subsystems move without any protocol change):

- **screen sharing** → `screen_sharing.proto` (10) + `screen_sharing_input.proto` (1) + `vnc.proto` (6) + `vnc_input.proto` (1)
- **auth** → `auth.proto` (`AuthService` 5, `LiveKitTokenService` 1) + `token.proto` (2). Only `MintLocalToken` is in `ConnectionService`
- **telegram** → not an RPC family at all: `tddy/v1/observer.proto` (`PresenterObserver`) + `tddy/v1/presenter_intent.proto` (9)
- **models** → `models.proto` (12) + `tddy/acp/v1/acp.proto` (1)
- **livekit** → only the observability stream `StreamLiveKitRooms` is here; token minting is `auth.LiveKitTokenService`

### the real cost of a proto split — cross-family shared messages

Referenced from more than one family, so each must live in a shared `.proto` (or be duplicated):
`SessionEntry` (:728), `ProjectEntry` (:810), `SessionContextDoc`/`Kind` (:700/:712),
`SessionAttachment` (:2703), `StagedAttachmentRef` (:2718), `HostDocumentRef`/`Scope` (:2731/:2752),
`SplitAgentPlacement` (:613), `AgentClonePlacement` (:620),
`SessionTerminalInput`/`Output` (:1691/:1719),
`ExecuteToolRequest`/`Response`/`Chunk` (:1860/:1868/:1878), `AgentActivityRecord` (:1948),
`StreamMode` (:1979), `WorktreeRow`/`WorktreeSizeStatus` (:1415/:1405), `Signal` (:1077),
`BranchConflict` (:1045), `ToolDef` (:1897), `ProbeOutcome` (:1147),
`BranchResolution` + `BranchBaseSync`/`BranchRemote`/`BranchSession`/`BranchWorktree` (:2400–2478).

**Four method pairs share a request type and therefore cannot be split across services without
duplicating it**: `StartSession`/`StreamStartSession`, `ReadWorktreeFile`/`StreamReadWorktreeFile`,
`ReadHostDocument`/`StreamReadHostDocument`, `ExecuteTool`/`StreamExecuteTool`.

### the codegen pipeline

```
packages/tddy-service/proto/*.proto
  ├── RUST — never committed ────────────────────────────────────────────
  │   packages/tddy-service/build.rs (418 lines, 21 passes)
  │     pass A  prost_build + tddy_codegen::TddyServiceGenerator
  │             → $OUT_DIR/connection.rs  (trait, 111 members; …Server<T> with const NAME;
  │                                        impl RpcService via method-name string dispatch)
  │             → tddy_service::proto::connection::*  (src/lib.rs:81-83)
  │             → tddy_service::ConnectionServiceServer (src/lib.rs:37)
  │     pass B  tonic_build + .extern_path(".connection", "crate::proto::connection")
  │             → $OUT_DIR/tonic_connection/connection.rs (server trait, Server<T>, Client;
  │                                                        NO duplicate messages)
  │             → tddy_service::tonic_connection::*  (src/lib.rs:251-258)
  │     pass C  descriptor-only over 19 protos → SERVICE_DESCRIPTOR_BYTES (gRPC reflection)
  │   NOTE packages/tddy-service/buf.gen.yaml is VESTIGIAL — src/gen does not exist
  └── TYPESCRIPT — committed ────────────────────────────────────────────
      cd packages/tddy-web && bunx buf generate ../tddy-service/proto
        buf.gen.yaml: v2, local protoc-gen-es, out: src/gen, opt: target=ts
        → src/gen/*_pb.ts  (26 tracked files; connection_pb.ts is 358,405 bytes)
      Three parallel, independent generators with the same layout:
        tddy-rust-typescript-tests → gen/     (frozen, badly stale)
        tddy-rpc-web               → src/gen/ (from ../tddy-rpc/proto)
        tddy-livekit-web           → src/gen/ (from ../tddy-livekit/proto)
```

Rust generated code is **never committed** (`$OUT_DIR` only). TypeScript **is** committed and
**nothing in CI regenerates or diffs it**. Two recorded drift hazards: regenerating
`tddy-rust-typescript-tests/gen` produces a **5,182-line diff** to its `connection_pb.ts` plus 12
never-committed files; `packages/tddy-web/src/gen/codex_oauth_pb.ts` exists with **no corresponding
`.proto`** (orphan), as does `tddy-livekit-web/src/gen/rpc_envelope_pb.ts`.

**A useful accident**: the generator is invoked over the whole proto *directory*, so splitting
`connection.proto` into N protos produces N `*_pb.ts` files with **no generator-config change at
all**.

### the three Rust "ConnectionService" types

1. `tddy_service::proto::connection::ConnectionService` — generated by `tddy-codegen`; the one the
   daemon implements (imported as `ConnectionServiceTrait`); 111 members; `tddy_rpc` wrappers.
2. `tddy_service::proto::connection::ConnectionServiceServer<T>` — generated `RpcService` adapter,
   `const NAME = "connection.ConnectionService"`.
3. `tddy_service::tonic_connection::connection_service_server::{ConnectionService, …Server<T>}` +
   `connection_service_client::ConnectionServiceClient` — generated by `tonic-build`.

**None is hand-written.** The only hand-written piece is the 1,560-line adapter implementing #3 by
delegating to #1. `tddy-coder` bypasses the trait entirely.

### transport matrix — what the service name is coupled to

| Transport | Mechanism | Coupling |
|---|---|---|
| Connect-HTTP `/rpc/{service}/{method}` | `tddy-connectrpc::connect_router` + `MultiRpcService` | `ServiceEntry.name` **string** |
| LiveKit data channel | `tddy_livekit::LiveKitParticipant` + `MultiRpcService` | same string; peer forwarding passes it explicitly at **12 sites** in `livekit_peer_discovery.rs` |
| Local Unix socket, native gRPC | `tonic::transport::Server` + `ConnectionServiceServer::new(adapter)` | **compile-time** |
| stdio (`tddy-stdio`) | `MultiRpcService` | string |
| Webview IPC (Tauri) | `tddy-tauri-rpc` `MultiConnectionHost` | string |
| Raw HTTP JSON (`tddy-discovery`) | `reqwest` POST to `{daemon_url}/connection.ConnectionService/ExecuteTool` | **hard-coded URL string** |
| gRPC reflection | `SERVICE_DESCRIPTOR_BYTES` + `reflection_entry_from(&names)` | descriptor set + name list |

**On every transport but the UDS one, adding a service is just a new `ServiceEntry` — no framework
change.**

### consumer map

| Consumer | What it does | Scale |
|---|---|---|
| **`packages/tddy-web`** | the dominant client | **244** `gen/connection_pb` imports (116 in 112 `src/` files, 130 in 126 `cypress/` files); **86 call sites in 45 `src/` files**; 6 hard-coded `ConnectionService` bindings in `src/rpc/`; a **736-line** `cypress/support/rpc/connectionServiceBackend.ts` fake plus ~10 siblings |
| **`packages/tddy-daemon`** | the primary server **and** a peer client | the 111-member impl; the 1,560-line tonic adapter; `local_socket_server.rs`; `runtime.rs:787-791`; **12** service-name literals in `livekit_peer_discovery.rs`; 105 method literals; **~85 test files**, 10 constructing the server directly |
| **`packages/tddy-coder`** | second, **partial** server | `session_participant/mod.rs` string dispatch over 14 methods (families K, L, M, N); everything else `Unimplemented` |
| **`packages/tddy-tools`** | in-jail agent client | 6 files, 30 method literals, 14 distinct methods |
| **`packages/tddy-sandbox-runner`** | **(service, method) relay allowlist** | `runner.rs:69` and `:89-94` — a tuple allowlist naming `ExecuteTool`, `StreamSessionAgents`, `OpenAgentConversation`, `PromptAgentConversation`, `CancelAgentConversation`, `ReportAgentConversationState` |
| **`packages/tddy-sandbox-app`** | tonic client | `daemon_client.rs` + a `sandboxed_session.rs:708` guard |
| **`packages/tddy-session-sync`** | client | `sync.rs`, `attach.rs` — `StreamAgentActivityDelta`, `ListSessions`, `ConnectSession` |
| **`packages/tddy-discovery`** | plain HTTP client | `tools.rs:141` builds the URL path by hand; 4 wiremock path assertions |
| `tddy-rpc`, `tddy-livekit-web` | framework tests only | the name used as an arbitrary fixture label |
| `tddy-rust-typescript-tests` | a committed, badly stale generated copy | — |

**Confirmed non-consumers** (zero hits): `tddy-e2e`, `tddy-integration-tests`, `tddy-tui`,
`tddy-desktop` (incl. `src-tauri`), `tddy-tauri-web`, `tddy-tauri-rpc`, `tddy-screenshare`,
`tddy-rpc-web`, `tddy-connectrpc`, `tddy-demo`, `tddy-demo-runner`, `tddy-supervisor`.

**Where the web client comes from — the seam to migrate**: `src/rpc/transportProvider.tsx:257`
`useHttpClient(service)` and `:331` `useLiveKitClient(service, room, identity)` are **service-generic**
`createClient(service, transport)` calls; `clientFor<S extends DescService>(service: S)` on
`HostConnection`/`SessionConnection` (`src/rpc/connections/types.ts:87-93`) is memoised per
connection per service. So **the transport layer needs no redesign — only the six hard-coded service
bindings and the import paths move.**

### `tddy-web` dependency resolution

Plain bun workspaces, symlinks, **no registry override**: root `package.json` lists 7 workspace dirs
(**`packages/tddy-service` is not among them** — tddy-web reaches its protos only by the relative
path in its `generate` script). `bun install` writes a single `bun.lock` and places workspace links
in the consumer's own `node_modules`:

```
packages/tddy-web/node_modules/tddy-connectrpc-testkit -> ../../tddy-connectrpc-testkit
packages/tddy-web/node_modules/tddy-livekit-web        -> ../../tddy-livekit-web
packages/tddy-web/node_modules/tddy-rpc-web            -> ../../tddy-rpc-web
packages/tddy-web/node_modules/tddy-tauri-web          -> ../../tddy-tauri-web
```

`bunfig.toml` has **no `[install]` block and no registry**; there is **no `.npmrc`** in the repo.
`tddy-rpc-web`, `tddy-tauri-web` and `tddy-connectrpc-testkit` all point `main`/`types` at
`./src/index.ts`; **`tddy-livekit-web` alone points at `./dist/`**, which is why `tddy-web` overrides
it in both `tsconfig.json` `paths` and `vite.config.ts` `alias` and builds it in `prebuild`.

The **local-registry path** the brief refers to:

```json
"resolve-local-lock":    "bun run scripts/resolve-local-lock.ts",
"local-install":         "scripts/local-bun-install.sh",
"local-registry-install":"bun run resolve-local-lock && bun run local-install"
```

`resolve-local-lock.ts` (default `LOCAL_REGISTRY_URL=https://npm.dev.wixpress.com`) reads
`bun.lock`, queries the registry per package, pins every range to an exact resolved version, writes
`local.bun.lock` and patched manifests into `.local-install/`. It **skips workspace deps explicitly**:

```ts
if (packageId.includes("@workspace:") || packageId.includes("link:") || packageId.includes("file:")) {
  return { key, entry, changed: false };
}
// and in resolveWorkspaceDeps:
if (depRange.startsWith("workspace:")) { newDeps[depName] = depRange; continue; }
```

`local-bun-install.sh` swaps `bun.lock` and the 8 patched manifests in place, `trap cleanup EXIT`
restores them, and runs `bun install --verbose --registry "$REGISTRY"`.

**Consequence for this stack**: if the generated TS split stays inside
`packages/tddy-web/src/gen/` (several `*_pb.ts`, one per new proto), **nothing in dependency
resolution changes at all** — only import paths move. Creating a *workspace package* per generated
service would each need a root `workspaces` entry, `main`/`types` → `./src/index.ts` (the
`tddy-rpc-web` pattern, **not** `tddy-livekit-web`'s `dist` pattern), a `workspace:*` entry in
tddy-web, a fresh `bun install`, and regeneration of `local.bun.lock` + `.local-install/`.

### wire-compatibility policy

**There is none written down.** Every search for a policy (`breaking change`, `wire compat`,
`backward compat`, `proto version`, `deployed client`, `version negotiat`, `lockstep`) returned only
per-change statements. The de-facto rule the changesets establish is **"break freely, migrate every
consumer in the same change"**:

- `docs/dev/changesets/2026-07-28-terminal-replay-viewport.md`: *"**Proto (not backward-compatible —
  `GrpcSessionTerminal` is the only consumer, updated in lockstep)**…"*
- `docs/dev/changesets/2026-07-02-removed-the-discovery-subagent-single-name-alias-entirely.md`:
  *"no backwards compatibility retained… field 19 — now `reserved`"* — the origin of
  `connection.proto:1031`.
- Counter-examples that preserved compatibility said so explicitly
  (`packages/tddy-service/docs/changesets/2026-08-02-vm-proto.md`: *"No field number is reused or
  renumbered … so the addition is wire-compatible"*).
- **The lockstep rule that bites this stack**, from
  `packages/tddy-coder/docs/changesets/2026-08-02-activities-tail-first-autoscroll.md`: *"the session
  participant serves tail mode and the cursor in lockstep with the daemon … had it not, the same
  session would have opened tail-first when reached over HTTP and head-first when reached over
  LiveKit."* **Any method in families K, L, M or N that moves must move in
  `tddy-coder/src/session_participant/mod.rs` in the same node.**

### the 14 surfaces a split has to touch

1. `packages/tddy-service/proto/connection.proto` — split the service block; place the ~25
   cross-family shared messages; keep the four request-sharing method pairs together.
2. `packages/tddy-service/build.rs` — one prost pass (+ one tonic pass where gRPC is needed) per new
   service; add each new proto to the descriptor list; **re-point the three `.connection.*` extern
   paths in the sandbox pass**.
3. `packages/tddy-service/src/lib.rs` — a `pub mod` `include!` and a `pub use …Server` per service.
4. `packages/tddy-daemon/src/connection_service.rs` — the 111-member impl becomes N impls.
5. `packages/tddy-daemon/src/connection_tonic_adapter.rs` — **one hand-written adapter per
   gRPC-served new service** (codegen cannot do this).
6. `packages/tddy-daemon/src/runtime.rs:788-791` — one `ServiceEntry` per service.
7. `packages/tddy-daemon/src/local_socket_server.rs` — one `.add_service()` per gRPC-served service.
8. `packages/tddy-daemon/src/livekit_peer_discovery.rs` — 12 hard-coded name literals.
9. `packages/tddy-coder/src/session_participant/mod.rs` — the `ServiceEntry` names and the string
   dispatch; families K/L/M/N in lockstep.
10. `packages/tddy-tools` (6 files, 14 methods), `packages/tddy-sandbox-runner/src/runner.rs` (the
    (service, method) allowlist), `packages/tddy-sandbox-app`, `packages/tddy-session-sync`,
    `packages/tddy-discovery/src/tools.rs` (hard-coded URL + 4 wiremock assertions).
11. `packages/tddy-web` — regenerate, then move 116 imports in 112 `src/` files and 130 in 126
    `cypress/` files, 86 call sites, 6 hard-coded bindings, and the 736-line fake + ~10 siblings.
12. `packages/tddy-daemon/tests/` — ~85 files, 10 constructing the server directly.
13. `packages/tddy-rust-typescript-tests/gen/connection_pb.ts` — already stale; regenerate or delete.
14. `packages/tddy-terminal-rpc/proto/terminal_session.proto` — **either actually serve it or retire
    it**; today it is the terminal family already extracted and wired to nothing.

### Findings

1. **`ConnectionService` is 90 methods in 20 identifiable families**, and the proto's own doc
   comments name the families — the grouping is not a guess. 69 unary, 21 server-streaming, 1 bidi.
2. **Five subsystems need no protocol change at all**, because they already have their own protos:
   models/ACP, screen sharing + VNC, auth + token, telegram (`observer`/`presenter_intent`), and the
   `tasks`/`actions`/`bsp`/`remote_git`/`session_admission`/`vm` services. Those crate moves are pure
   relocation — the cheapest and best-first nodes.
3. **The terminal family is already extracted and serving nothing.** `terminal_session.proto`
   duplicates family K exactly, is served nowhere, and its consumers hand-convert between the two
   message sets. This is both a free node and the cautionary precedent: **extracting a proto without
   moving the served coordinate leaves dead code plus a converter.**
4. **The cross-family shared messages are the real cost.** ~25 messages are reached from more than
   one family, and four method pairs share a request type outright. A shared `types.proto` imported by
   every new service is the only option that does not duplicate them.
5. **Three `.connection.*` extern paths in the sandbox tonic pass will break** unless re-pointed —
   a compile failure with a confusing message, worth naming in the affected node.
6. **Adding a service is a one-line `ServiceEntry` on every transport but the UDS/tonic one**, where
   it costs a hand-written adapter per service (the codegen's `generate_tonic_adapter` is a stub).
7. **The web transport layer needs no redesign** — `useHttpClient(service)` / `clientFor(service)`
   are already service-generic. The migration is import paths, 86 call sites, six hard-coded
   bindings, and the Cypress fakes.
8. **Splitting the proto costs nothing in bun dependency resolution** if the generated TS stays in
   `packages/tddy-web/src/gen/` — the generator already runs over the whole proto directory, so N
   protos produce N `*_pb.ts` automatically.
9. **`tddy-coder` is a second server for families K/L/M/N and must move in lockstep**, or the same
   session answers differently over HTTP and over LiveKit — a failure mode the repo has already hit
   once and documented.
10. **There is no wire-compatibility policy and no CI codegen-drift gate.** The de-facto rule is
    "break freely, migrate every consumer in the same change", which is exactly this stack's licence —
    and `connection.proto` uses a bare `package connection;`, so **the split is the last cheap chance
    to introduce versioned package names**.


## Exploration 7: what nodes 1–8 leave in `tddy-daemon` — 2026-09-10

**Agent**: parent Bash (python over the module tree, cross-referenced with each node's changeset)
**Scope**: subtract every module claimed by nodes 1–8 from the daemon's 106 modules, and classify the
59 files of `connection_service/` by which family's handler they are.

### Sequence

1. Walk `packages/tddy-daemon/src`, counting non-blank lines per file — 106 flat modules plus 59 files
   under `connection_service/`.
2. Build the claimed-module set from each node's changeset `## Responsibility` / `## Affected Packages`.
3. Subtract, to get the residual.
4. Classify `connection_service/`'s 59 files by family keyword (`start_session`, `resume`,
   `delete_session`, `project`, `demo_vm`, `local_token`, …) to split family C/D/O/Q handlers from the
   rest.

### Findings

| Residual after nodes 1–8 | Lines |
|---|---|
| Genuine wiring (`main`, `lib`, `server`, `startup`, `runtime`, `config`) | 3,699 |
| Session lifecycle (9 modules) | 6,240 |
| Family C/D/O/Q handlers in `connection_service/` (25 files) | 6,491 |
| UDS transport (`connection_tonic_adapter` 1,437, `local_socket_server` 178) + facade 2,422 | 4,037 |
| Daemon's own config service + small utilities | 1,012 |
| **Total** | **≈21,500** |

1. **The eight-node endpoint is not wiring.** 21,500 lines against 3,699 of actual wiring. The brief
   said *"leave them only for high-level wiring"*.
2. **`ConnectionServiceImpl` survives the eight-node plan intact**, because family C is precisely what
   needs its 60 fields, its 21 `with_*` builders and its `self_arc` handle. The stack would have moved
   73 of 90 methods and left the architectural defect it was aimed at.
3. **Two changesets contradict each other.** Node 1's `## Affected Packages` gives
   `tddy-worktree-service` the 8-module git/worktree subsystem, which includes `project_storage.rs`
   and `project_provision.rs`; node 8's `## Boundaries` declares those two as staying with family D.
4. **25 of `connection_service/`'s 59 files are family C/D/O/Q handlers** — 6,491 lines, the largest
   single being `svc_start_session_core.rs` at 847.
5. **The recorded pre-existing failure has a shelf life.**
   `self_arc called before set_self_handle` has been carried as pre-existing through every changeset in
   this stack. It exists only because `ConnectionServiceImpl` exists, so the node that deletes the god
   object is the node where that failure count must drop to 0 — and therefore where it must be asserted
   rather than inherited.

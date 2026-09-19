# Initial Discovery: Sandboxed codebase mode from the web

**Changeset**: [2026-09-18-sandboxed-codebase-mode-from-the-web.md](./2026-09-18-sandboxed-codebase-mode-from-the-web.md)
**Date**: 2026-09-18
**Passes**: 3

## Combined Conclusions

### The thing being asked for

`--codebase-mode sandboxed` (`docs/ft/coder/sandboxed-codebase-mode.md`, landed as #447) is the
**inverted placement**: the checkout is mounted read-write into a jail and the *agent* runs on the
**host**, unconfined, with every native filesystem and shell tool withdrawn (`--disallowedTools`),
reaching the code only through `mcp__tddy-tools__*` calls dispatched **into** the jail.

It exists only in `tddy-sandbox-app`, only on macOS, and only as a CLI flag. The request is to make
it reachable from `tddy-web`.

### What the web already has, and what it does not

**Already present.** `CreateSessionPane.tsx` has a `Sandbox` checkbox (`sandbox` state,
`create-session-sandbox-toggle`) and a `Codebase host` select (`codebaseDaemonInstanceId`). On a
**split** placement (agent on daemon A, checkout on daemon B), `sandbox = true` already produces the
inverted shape *across two hosts*: A runs claude-cli unconfined with managed-codebase argv, B holds
the checkout inside a `--workspace-tools` jail. This landed in
`docs/dev/1-WIP/2026-08-31-split-sandbox-orchestration.md` and
`docs/ft/web/changelog/2026-08-31-sandbox-toggle-on-a-split-managed-codebase-session.md`.

**Missing.** The **co-located** inverted placement — one daemon, agent unconfined on it, that same
daemon's checkout in a jail. Today on one host the web can produce only:

| Web form state | Effective placement |
|---|---|
| `sandbox`off, `managedCodebase` off | agent on host, repo on host, nothing confined |
| `sandbox` on, `managedCodebase` off | agent **in** jail, repo in jail (`mounted`) |
| `sandbox` on, `managedCodebase` on | agent **in** jail, repo on host (`managed`) |
| — no combination — | agent on host, repo **in** jail (`sandboxed`) ← the gap |

### The daemon has no notion of a codebase *mode*

`grep -rn "CodebaseMode\|codebase_mode" packages/tddy-daemon/src packages/tddy-session-lifecycle/src
packages/tddy-daemon-sandbox/src` returns **nothing**. The wire carries two independent booleans:

```proto
bool sandbox = 16;            // session.proto:411-413
bool managed_codebase = 17;   // session.proto:414-417
```

`tddy-sandbox-app`'s Linux path collapses the three modes onto that boolean pair and **refuses**
the third outright, client-side, before any RPC is made:

```rust
CodebaseMode::Sandboxed => Err(
    "--codebase-mode sandboxed is supported only on macOS: it needs a --workspace-tools \
     jail this app provisions itself, which the Linux daemon-assisted path cannot yet do"
        .to_string(),
),
```
— `packages/tddy-sandbox-app/src/codebase_mode.rs:116-120`

Its doc comment is the crux: *"The daemon would have to provision it, and does not yet know how
to."*

### The daemon is closer than that refusal suggests

It already owns every jail-side piece:

| Piece | Coordinate |
|---|---|
| `--workspace-tools` jail provisioner | `packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs:250-315` (`JailedWorkspaceSandboxProvisioner`) |
| Declared **Linux-capable** | `workspace_tool_sandbox.rs:233-243` — `Ok(())` on both macOS and Linux |
| Per-session jail registry | `WorkspaceSandboxRegistry`, `workspace_tool_sandbox.rs:500-521` |
| Routing choke point | `exec_tool_route`, `packages/tddy-session-lifecycle/src/svc_resolve_os_user.rs:218-247` |
| In-jail exchange | `exchange_in_jail_tool_call`, `workspace_tool_sandbox.rs:465-496` |
| Teardown / orphan reaping | `RUNNER_PID_FILE`, `session_deletion.rs:116, 613` |

What it does **not** have is enumerated in Exploration 1 § 8 as seven gaps. The load-bearing ones:

1. **No wire field for the third mode.** `daemon_client.rs:173` derives
   `managed_codebase: codebase_mode == "managed"`, so `sandboxed` would silently become an ordinary
   mounted session — exactly the outcome the refusal exists to prevent.
2. **The daemon's jail shape differs from the app's.** `build_workspace_tool_plan`
   (`workspace_tool_sandbox.rs:149-195`) hardcodes `--stdio`, `loopback_allow_ports: vec![]`, **no**
   `--egress-shim-port`, and mounts **only** the worktree. A `sandboxed` jail additionally needs an
   egress shim + its loopback allow, a second read-write mount for the per-repository build `$HOME`,
   and that home's ancestor `metadata` grants (`sandboxed_session.rs:447-454`). None are parameters
   on `WorkspaceToolPlanRequest` today.
3. **`exec_tool_route` jail-routes only `session_type == "workspace"` with `sandbox == Some(true)`**
   (`svc_resolve_os_user.rs:235-236`). A co-located `claude-cli` session in sandboxed mode would
   route to `HostWorktree` — i.e. run every tool call unconfined, under a name promising
   confinement.
4. **No host-run-agent lifecycle.** `tddy_daemon_sandbox::sandbox_session` always puts the agent
   *inside* the jail. The unconfined-agent argv builder (`host_agent::build_host_agent_argv`) is
   `#[cfg(target_os = "macos")]` and lives in the **app** crate.
5. **Terminal model.** The CLI's `sandboxed` agent inherits the operator's real TTY — there is no
   PTY, no terminal bridge, and `log_destination` diverts the app's own logs to a file precisely
   because the agent owns the terminal (`main.rs:214-219`). A web-started session has no operator
   terminal on the daemon host: the daemon must give the unconfined agent a **PTY it owns** and
   stream it over `StreamSessionTerminalIO`. This is a genuine departure from the CLI feature, not
   a port of it.

### The design fork this planning has to settle

There are two credible implementations, and they differ by an order of magnitude:

**(A) Port the app's path into the daemon.** Lift `sandboxed_session.rs` + `host_agent.rs` out of
`tddy-sandbox-app` into a shared crate, make them non-macOS, and give the daemon a fourth session
shape. Closes gaps 1–5 with new daemon code.

**(B) Express co-located sandboxed as a self-split.** The daemon already does exactly this shape
across two hosts. `codebase_daemon_instance_id == daemon_instance_id` is currently classified as
*co-located* by both the proto comment (`session.proto:462-471`) and the web predicate
(`CreateSessionPane.tsx:243-276`, which deliberately treats naming your own host as co-located).
Reclassifying that case — a **local** `workspace` session with `sandbox = true` holding the
worktree, plus a local claude-cli agent with managed-codebase argv routed at it — reuses the whole
split orchestration, the existing jail, the existing routing, and the existing resume/teardown.

(B) is materially smaller and is what the split machinery was built for; it inherits the split
session's real limitations (recipe withdrawn, `--dangerously-skip-permissions` withdrawn,
claude-cli only). (A) reproduces the CLI feature exactly, including the egress shim and the
per-repository build `$HOME`, which (B) does **not** get for free — the daemon's jail has neither.

### Linux

Both the architecture doc and the feature PRD record the Linux cgroups jail as
**implemented but never run end to end**:

> "is implemented and unit/transport-integration-tested, but has **not** been run end-to-end (no
> automated test drives a live daemon; the on-host run also depends on the daemon's own cgroups
> sandbox, which has open issues)"
> — `packages/tddy-sandbox/docs/architecture.md:102-105`

And three standing backlog items bear directly on the Linux confinement claim:

- `docs/dev/todo/2026-06-28-tddy-sandbox-cgroups.md` — **the Linux jail still shares the host
  filesystem root**; `pivot_root` into a minimal RO root is unbuilt. A jailed build can therefore
  still write outside the checkout.
- `docs/dev/todo/2026-07-02-tddy-sandbox-cgroups.md` — `--stdio` jail-spawn piping on Linux is
  compile-checked only, never run in a real jail.
- `docs/dev/todo/2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md`
  — the self-skip probe answers an easier question than the jail asks, so Linux sandbox tests fail
  instead of skipping on hosts with `apparmor_restrict_unprivileged_userns=1`.

### Test surface

- **Web**: Cypress component specs only. `mountWithRpc` + `anInMemoryRpcBackend` from
  `tddy-connectrpc-testkit`; all selectors behind `cypress/support/pages/createSessionPage.ts`
  ("No raw `cy.get(...)` in test files"). The nearest neighbours are
  `CreateSessionCodebaseHostAcceptance.cy.tsx` (22 `it`s), `CreateSessionSandboxToggle.cy.tsx`,
  `CreateSessionSplitSshAcceptance.cy.tsx`. There is **no** e2e spec for the create form.
- **Daemon**: `packages/tddy-daemon/tests/workspace_tool_sandbox_acceptance.rs` and
  `…_seatbelt_acceptance.rs`; `remote_managed_worktree_cross_host_acceptance.rs` for the split path.
- **App**: `packages/tddy-sandbox-app/tests/sandboxed_codebase_seatbelt_acceptance.rs` (931 lines)
  is the existing proof of the mode.

---

## Exploration 1 — `--codebase-mode sandboxed` as implemented

**Why**: establish exactly what the feature is, what refuses it, and what a daemon would need.

**Sequence**: read `docs/ft/coder/sandboxed-codebase-mode.md`; `packages/tddy-sandbox-app/src/{main,codebase_mode,sandboxed_session,host_agent,spawn,bridge,config}.rs`;
`packages/tddy-daemon-sandbox/src/workspace_tool_sandbox.rs`;
`packages/tddy-sandbox-recipes/src/claude_cli.rs`;
`packages/tddy-sandbox-runner/src/{host_relay,runner,main}.rs`;
`packages/tddy-sandbox/docs/architecture.md`.

### Findings

**The three modes** (`sandboxed-codebase-mode.md:16-54`):

| `--codebase-mode` | Agent | Checkout | Jail confines |
|---|---|---|---|
| `mounted` (default) | jail | jail (rw) | the agent + the code with it |
| `managed` | jail | host (never mounted) | the agent; code via host-relayed `mcp__tddy-tools__*` |
| `sandboxed` | **host, unconfined** | **jail (rw)** | the codebase and every build/test/tool call |

Architecture, `sandboxed-codebase-mode.md:41-54`:

```
host                                          jail (Seatbelt)
claude  (real TTY, ~/.claude, network)
  └─ mcp__tddy-tools__{Read,Write,Shell,…}
       └─ tddy-tools --mcp
            └─ TDDY_SANDBOX_TOOL_IPC ──┐
   tddy-sandbox-app                    │
     ├─ tool IPC socket  <─────────────┘
     ├─ host relay ── in_jail_tool_request ──> tddy-sandbox-runner --workspace-tools <repo>
     │                <── in_jail_tool_response ──   └─ tddy_tool_engine over the mounted repo
     └─ CONNECT tunnels <──────────────────────────  └─ egress shim (cargo fetch, bun install)
```

**15 acceptance criteria** (`:246-313`). Criterion 10 is the one this change targets:
*"`--codebase-mode sandboxed` on Linux is refused, naming macOS; the Linux daemon-assisted path is
otherwise unchanged."*

**Deliberately out of scope in #447** (`:315-329`): Linux; cursor; consolidating the daemon's own
in-jail dispatch; workflow recipes on a jailed checkout; `--cwd`.

**CLI flags** — `main.rs:48-180`. The two that define the mode:

```rust
    #[arg(long, env = "TDDY_SANDBOX_CODEBASE_HOME")]
    codebase_home_dir: Option<PathBuf>,     // main.rs:128-129
    #[arg(long)]
    codebase_mode: Option<String>,          // main.rs:145-146
```

**Mode resolution** — `codebase_mode.rs:19-28`, `:36-53`:

```rust
pub enum CodebaseMode { Mounted, Managed, Sandboxed }
pub fn resolve_codebase_mode(codebase_mode: Option<&str>, remote_codebase_flag: bool)
    -> Result<CodebaseMode, String>
```

**The fork point** — `main.rs:568-585`:
`if mode == CodebaseMode::Sandboxed { return run_sandboxed_codebase(SandboxedCodebaseRun {...}).await; }`
— "The inverted placement forks the flow here and never rejoins it."

**Log destination diverges** — `main.rs:214-219`: `Sandboxed` ⇒ file (`sandbox-app.log`);
`Mounted | Managed` ⇒ stderr, because in `sandboxed` the agent owns the terminal.

**Provisioning** — `sandboxed_session.rs`:

```rust
pub struct SandboxedCodebaseParams {
    pub repo: PathBuf, pub session_id: String, pub session_dir: PathBuf,
    pub sandbox_runner_path: Option<String>, pub tddy_tools_path: Option<String>,
    pub repo_build_home: PathBuf,
}                                                            // :138-169
pub fn repo_build_home(base: &Path, canonical_repo: &Path) -> PathBuf {
    base.join(tddy_core::session_actions::derive_repo_key(canonical_repo))
}                                                            // :129-131
pub async fn provision_with_interrupt(params, interrupt) -> Result<SandboxedCodebaseSession>
                                                             // :304-511
```

Constants: `JAIL_READY_TIMEOUT = 120s` (`:54`), `TOOL_SOCKET_MODE = 0o600` (`:112`).
Ancestor metadata grants, `:447-454`:

```rust
    for ancestor in repo.ancestors().skip(1).chain(build_home.ancestors().skip(1)) {
        plan.reads.push(ReadSpec::metadata(ancestor, ReadReason::Custom));
    }
```

Relay wiring, `:580-586`:

```rust
    let (relay, dispatcher) = run_host_relay_with_in_jail_tools(
        client, NullToolHandler, HostRelayConfig::new(args.session_id, terminal_tx), stdin_rx,
    ).await
```
— `NullToolHandler` "is not a stub here, it is the truth" (`:577-579`).

Host tool socket serves **only** `ExecuteTool`, `:719-721`:
`Status::not_found("a sandboxed codebase session serves only ExecuteTool, got {service}/{method}")`.

**Host agent argv** — `host_agent.rs:126-195`, `build_host_agent_argv`: base argv →
`--strict-mcp-config` → `--setting-sources user` → pass-through `claude_args` →
`append_host_agent_mcp_args`. `host_mcp_env` (`:80-119`) sets `TDDY_SANDBOX_TOOL_IPC`,
`TDDY_TOOLS_LOG_FILE`, `TDDY_TOOLS_ACCOUNTING_FILE`, `RUST_LOG`.

**Disallowlist** — `packages/tddy-sandbox-recipes/src/claude_cli.rs:314-327`:

```rust
pub fn build_host_agent_disallowlist() -> Vec<String> { /* Read Write Grep Glob + native aliases */ }
```
with `native_aliases` (`:185-191`) mapping `"Shell" => ["Bash","BashOutput","KillShell"]`,
`"Write" => ["Edit","MultiEdit","NotebookEdit"]`.

**Runner argv builders — app vs daemon, side by side.**

App, `spawn.rs:312-354` (`build_workspace_tools_runner_argv`): `--session-id`, `--context-dir`,
`--tool-ipc-socket`, `--tddy-tools-path`, `--ready-marker`, `--grpc-socket`,
`--workspace-tools <repo>`, **`--grpc-listen-port`**, **`--egress-shim-port`**.

Daemon, `workspace_tool_sandbox.rs:149-164`: `--session-id`, `--context-dir`, `--tool-ipc-socket`,
`--tddy-tools-path`, `--ready-marker`, `--workspace-tools <worktree>`, **`--stdio`**.

Plan differences: daemon has `loopback_allow_ports: vec![]` (`:183`),
`mounts: vec![MountSpec::read_write(worktree_path)]` — **no build `$HOME`** (`:190`),
`recipe: Some(SandboxRecipe::Shell)`, `sysctl_read = true` (`:213`).

Mounts builder, app side, `spawn.rs:274-285`:

```rust
pub(crate) fn build_sandbox_mounts(mode: CodebaseMode, repo: &Path, scratch_home: &Path)
    -> Vec<tddy_sandbox::MountSpec> {
    let mut mounts = Vec::new();
    if mode != CodebaseMode::Managed { mounts.push(MountSpec::read_write(repo)); }
    mounts.push(MountSpec::read_write(scratch_home));
    mounts
}
```

**Transport resolution** — `runner.rs:2072-2104`:

```rust
pub enum WorkspaceToolsTransport { Stdio, LoopbackGrpc(u16) }
pub fn resolve_workspace_tools_transport(stdio: bool, grpc_listen_port: Option<u16>)
    -> Result<WorkspaceToolsTransport, String>
```
`(false, None)` errors naming both flags.

**Egress** — `runner.rs:1730-1747` `egress_proxy_env`: sets `HTTPS_PROXY`/`HTTP_PROXY`/lowercase to
`http://127.0.0.1:<port>` plus `NO_PROXY=127.0.0.1,localhost`; **empty vec when no shim**, with the
comment "the daemon's `--workspace-tools` jail holds only a checkout, needs no network".

**Dispatcher** — `host_relay.rs:294-304`, `IN_JAIL_TOOL_TIMEOUT = 600s` (`:276`, shared with the
daemon deliberately). `InJailToolDispatcher::execute` (`:328-392`) serialises on a `turn` mutex and
`declare_lost()`s on timeout.

**Architecture doc** — `packages/tddy-sandbox/docs/architecture.md`:

§ Standalone on Linux (`:89-105`), verbatim reason:

> "an unprivileged app cannot place its own child in a limited cgroup scope (cgroup v2 **delegation
> containment** — the common ancestor of its shell scope and any writable delegated subtree is the
> root cgroup, which it can't write). So on Linux `tddy-sandbox-app` **delegates to a running
> `tddy-daemon`**"

§ The two `--workspace-tools` callers (`:107-127`):

| Caller | Transport | Egress |
|---|---|---|
| `tddy-daemon`, for a sandboxed `workspace` session | `--stdio` | none |
| `tddy-sandbox-app`, for `--codebase-mode sandboxed` | `--grpc-listen-port` | `--egress-shim-port` |

`ReadSpec::Metadata` (`:168`) is lookup-only on macOS and **no bind mount at all** on Linux.

### Every refusal on the path

| # | Refusal | Site |
|---|---|---|
| R1 | `sandboxed` + `--remote-codebase` | `codebase_mode.rs:44`, `:58-63` |
| R2 | Unknown `--codebase-mode`, naming all three | `codebase_mode.rs:46-49` |
| R3 | **`Sandboxed` on the daemon path — "only on macOS"** | `codebase_mode.rs:116-120` ← called `main.rs:383-384` |
| R4 | `--codebase-home-dir` outside `sandboxed` | `codebase_mode.rs:88-103` |
| R5 | `sandboxed` + `--agent-kind cursor` | `main.rs:719-737` |
| R6 | `sandboxed` + `--cwd` | `main.rs:747-758` |
| R7 | Inline `subagents:` on Linux | `main.rs:401-407` |
| R8 | A def that `replaces: [Shell]` and binds `SHELL` | `config.rs:132-141` |
| R9 | Non-macOS, non-Linux host | `main.rs:368-371` |
| R10 | Jail without `--workspace-tools` answering `in_jail_tool_request` | `runner.rs:2238-2247` |
| R11 | `--workspace-tools` with neither transport flag | `runner.rs:2097-2102` |
| R12 | Anything but `ExecuteTool` on the app's host socket | `sandboxed_session.rs:719-721` |
| R13 | A daemon session that is not `workspace` + `sandbox: true` is never jail-routed | `svc_resolve_os_user.rs:235-236` |
| R14 | A `workspace` session marked sandboxed with no jail → refuse, never fall back to host | `svc_resolve_os_user.rs:240-245` |

**R3 is the single gate**, and it fires client-side before any RPC.

### The seven daemon gaps

1. No wire representation for a third mode (`session.proto:411-417` has two booleans);
   `daemon_client.rs:173` would silently downgrade `sandboxed` to a mounted session.
2. `build_workspace_tool_plan` cannot express the jail shape (no egress shim, no build-home mount,
   no loopback allow, no `--grpc-listen-port`); `WorkspaceToolPlanRequest` (`:115-122`) has no
   fields for them.
3. No host-run-agent lifecycle; `build_host_agent_argv` is macOS-only and in the app crate.
4. No `InJailToolDispatcher` on the daemon's relay (it uses `run_host_relay_with_rpc` and its own
   raw channel) and no CONNECT-tunnel fulfilment on the tool-call channel.
5. No terminal model for an unconfined host agent.
6. `exec_tool_route` jail-routes `workspace` sessions only (`svc_resolve_os_user.rs:235-236`).
7. Linux end-to-end confidence: architecture doc `:102-105`, PRD `:317-319`.

---

## Exploration 2 — `tddy-web` session creation and split sessions

**Why**: establish the surface the flag has to arrive on and the house test style.

**Sequence**: route + drawer files; `CreateSessionPane.tsx` in full; `grep -i split` across
`packages/tddy-web/src` and `docs/ft/web/`; `session.proto`; `src/gen/session_pb.ts`;
`git show --stat d6274d3f`; `cypress/component/` listing; `packages/tddy-web/docs/`.

### Findings

**Route**: `SESSIONS_NEW_ROUTE = ${SESSIONS_DRAWER_ROUTE}/new` (`src/routing/appRoutes.ts:137`),
`isSessionsNewPath` (`:139-141`), consumed at
`src/components/sessions/SessionsDrawerScreen.tsx:140`.

**Components**: `CreateSessionPane.tsx` (**1412 lines**), `CreateSessionDialog.tsx`,
`CreateSessionAgentSelect.tsx`, `CreateSessionSshConfigSelect.tsx`,
`createSessionFormStyles.ts` (`inputClass` / `labelClass`), attachments sub-UI,
`BranchConflictDialog.tsx`.

**Form state**, `CreateSessionPane.tsx:172-283` — the fields that matter here:

```tsx
  const [sandbox, setSandbox] = useState(false);
  const [managedCodebase, setManagedCodebase] = useState(false);
  const [semanticIndex, setSemanticIndex] = useState(false);
  // Which daemon's filesystem holds the worktree. Empty means "same as host" …
  const [codebaseDaemonInstanceId, setCodebaseDaemonInstanceId] = useState("");
  // OpenSSH Host alias the exec catalog runs on. Empty is LocalShell on the code-managing host.
  const [sshConfigHost, setSshConfigHost] = useState("");
```

**The split predicate**, `:243-276`:

```tsx
  const canChooseCodebaseHost =
    sessionType === "claude-cli" && managedCodebase && daemons.length > 0;
  const isSplitCodebase =
    canChooseCodebaseHost &&
    codebaseDaemonInstanceId !== "" &&
    // Naming the session's own host is the explicit spelling of "co-located", and the daemon
    // classifies it exactly that way. …
    codebaseDaemonInstanceId !== daemonInstanceId;
```

**The claude-cli request**, `:604-634`:

```tsx
    return {
      ...commonParams,
      toolPath: "", agent: "",
      recipe: managedCodebase && !isSplitCodebase ? recipe : "",
      stackParent, stackParentDaemonInstanceId, stackNodeId,
      sessionType: "claude-cli", model, permissionMode,
      dangerouslySkipPermissions: isSplitCodebase ? false : dangerouslySkipPermissions,
      initialPrompt, sandbox, managedCodebase,
      specializedAgents: selectedAgentIds, semanticIndex,
      codebaseDaemonInstanceId: isSplitCodebase ? codebaseDaemonInstanceId : "",
      sshConfigHost,
    };
```

**The Sandbox checkbox and its comment**, `:1069-1084`:

```tsx
          {/* On a co-located placement the sandbox confines the agent on this daemon. On a split
              placement it confines the codebase host — the jail runs on the daemon holding the
              checkout, not the agent host. The combination with codebase_daemon_instance_id is
              admitted and the jail is placed on the codebase host. */}
```

**What a split withdraws**: Recipe (`:1128-1147`), `dangerouslySkipPermissions` (`:1050-1066`).
**Keeps**: Sandbox, specialized agents, semantic index, SSH host (retargeted at B).

**Placement table**, `docs/ft/daemon/remote-managed-worktree.md:51-55`:

| `daemon_instance_id` | `codebase_daemon_instance_id` | Result |
|---|---|---|
| `A` | empty | Co-located. Exactly today's behaviour. |
| `A` | `A` | Co-located. Explicit form of the above. |
| `A` | `B` | **Split.** Agent on A, worktree on B. |

**Proto**, `packages/tddy-service/proto/session.proto` — `StartSession` / `StreamStartSession`
(`:26-32`), `StartSessionRequest` (`:376-548`):

```proto
  bool sandbox = 16;                          // :411-413
  bool managed_codebase = 17;                 // :414-417
  string codebase_daemon_instance_id = 32;    // :462-471
  string requested_session_id = 33;           // :472-483
  AgentClonePlacement agent_clone = 34;       // :503
  SplitAgentPlacement split_agent = 35;       // :529
  string ssh_config_host = 38;                // :534-541
```

**Generated TS**: `packages/tddy-web/src/gen/session_pb.ts` — `StartSessionRequest` `:1000-1345`
(`managedCodebase` `:1136`, `codebaseDaemonInstanceId` `:1240`, `sshConfigHost` `:1340`).
The web builds a `MessageInitShape`, never a message:
`StartSessionRequestInit` (`src/hooks/useSessionAttachments.ts:48`), produced by
`startSessionRequest()` and consumed at `CreateSessionPane.tsx:660-664`.

**`d6274d3f`** (#486, split SSH 4/4) — 12 files, +667/−75. Web: new
`cypress/component/CreateSessionSplitSshAcceptance.cy.tsx` (+165), `createSessionPage.ts` (+4),
`CreateSessionPane.tsx` (39±), `CreateSessionSshConfigSelect.tsx` (178±),
`packages/tddy-web/docs/changesets/2026-09-14-split-ssh.md`. Introduced the pure
`sshConfigListDaemonId(sessionHostDaemonId, codebaseHostDaemonId)` helper.

**Test style** — mandatory shape, from `CreateSessionSplitSshAcceptance.cy.tsx`:
`anInMemoryRpcBackend()` with `.onUnary(Service.method.x, handler)` / `.implement(Service, {…})`;
`mountWithRpc(jsx, backend)`; `aJoinedCommonRoom([...])` + `SelectedDaemonProvider`;
page object `createSessionPage`; Given/When/Then comments in every `it`; typed request assertions
via `backend.callsTo(SessionService.method.startSession)` behind a named helper:

```tsx
function theStartSessionRequest(backend: InMemoryRpcBackend) {
  const calls = backend.callsTo(SessionService.method.startSession);
  expect(calls, "exactly one StartSession must have been sent").to.have.length(1);
  return calls[0];
}
```

Page-object rule, `cypress/support/pages/createSessionPage.ts:1-5`: *"All raw selectors live here;
test bodies call named methods. No raw `cy.get(...)` in test files."*

**Existing create-session specs**: `CreateSessionPane.cy.tsx` (29), `CreateSessionAcceptance` (15),
`CreateSessionCodebaseHostAcceptance` (22), `CreateSessionSandboxToggle` (2),
`CreateSessionManagedCodebase` (4), `CreateSessionSplitSshAcceptance` (2), plus ~18 more.
**No e2e spec covers the create form** (only `e2e/terminal-rendering.cy.ts:15-55` drives it
incidentally).

**Web changelogs** naming this area: `docs/ft/web/changelog/2026-08-14-choosing-which-daemon-holds-a-session-s-codebase.md`,
`…/2026-08-31-sandbox-toggle-on-a-split-managed-codebase-session.md`,
`…/2026-09-14-split-ssh-create-session.md`.

---

## Exploration 3 — deferred-work records and adjacent WIP (Step 2b)

**Why**: find items sitting in this change's path before the changeset is written.

**Sequence**: `ls -d packages/*/docs/code-issues/`; `grep -rn 'Claimed by:' packages/*/docs/code-issues/`;
`grep -rln 'Restructure:\*\* required'`; `ls docs/dev/todo/ | sort -r`; read the sandbox/daemon/web
candidates; `gh pr view 498`; `gh pr list --search carve`; `ls docs/dev/1-WIP/`.

### Findings

**Packages with a `docs/code-issues/` directory**: `tddy-code-restructuring`, `tddy-core`,
`tddy-daemon`, `tddy-session-lifecycle`, `tddy-workflow-recipes`. **Every package this change
touches except `tddy-daemon` has never been analyzed** — `tddy-web`, `tddy-sandbox-app`,
`tddy-daemon-sandbox`, `tddy-sandbox`, `tddy-sandbox-cgroups`, `tddy-sandbox-darwin`,
`tddy-sandbox-runner`, `tddy-sandbox-recipes`, `tddy-service` have no directory. Not analyzed is
not the same as clean.

**Claimed issues** — all 14 belong to the `#carve` stack. The two in this change's path:

```
packages/tddy-daemon/docs/code-issues/misplaced-tests-integration-suites.md
  Claimed by: #498 — #carve 4/10 test-homes · draft · feature/carve/test-homes
  Lands after: #488 (merged), #489 (merged), #490 (open)
packages/tddy-daemon/docs/code-issues/heavy-dependency-tests-only-runtime-deps.md
  Claimed by: #498 · same chain
```

`gh pr view 498` → `OPEN`, `isDraft: true`, base `feature/carve/restructure-clusters` (#490).
`#488` and `#489` are **merged** (they are in `git log`), so #498's only open predecessor is #490.

The issue pre-answers the fork itself, § *If you are about to change this code*:

> - **Adding a test to `tddy-daemon/tests/`**: fine, and usually right — #498 will route it to the
>   owning crate with the rest. Say which crate it actually exercises.
> - **Editing an existing suite**: fine; the move is mechanical and will carry your edit.
> - **Relying on `tddy_daemon::` re-export paths in new production code**: don't. #498 deletes the
>   facade.

**TODO backlog hits** (read in full): `2026-09-05-from-2026-09-05-sandboxed-codebase-mode.md`,
`2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md`,
`2026-06-28-tddy-sandbox-cgroups.md`, `2026-07-02-tddy-sandbox-cgroups.md`,
`2026-08-02-unprivileged-userns-available-under-approximates-what-the-jail-needs.md`,
`2026-09-15-the-daemon-orphans-its-sandbox-children-on-shutdown.md`,
`2026-09-09-daemon-sandbox-suites-never-call-set-self-handle.md`,
`2026-09-12-the-in-jail-conversation-suite-runs-nowhere.md`,
`2026-08-19-session-creation-agent-catalog-the-rest-of-the-fan-out.md`.

The decisive quote, from `2026-09-05-from-2026-09-05-sandboxed-codebase-mode.md`:

> **Linux `sandboxed` mode.** `run_linux` delegates to a running `tddy-daemon`; carrying a third
> codebase mode over `StartSessionRequest` and provisioning the cgroups equivalent of the
> `--workspace-tools` jail is a daemon-side change on a path documented as not verified end-to-end.

**Adjacent WIP changesets** — `docs/dev/1-WIP/2026-08-31-split-sandbox-orchestration.md` and
`…-resume.md`. Both fully checked off (landed, not yet wrapped). Their `## Boundaries` read:

> Does **not** change co-located sandbox or allow `recipe` on split.

— i.e. the co-located inverted placement was scoped out there by name, and is exactly this
changeset's territory. No conflict; they are the machinery this change either reuses or parallels.

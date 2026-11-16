# Initial Discovery: Derived hosts are offered under the daemon the web addresses

**Changeset**: [2026-11-16-remote-shell-derived-host-identity.md](./2026-11-16-remote-shell-derived-host-identity.md)
**Stack**: `#remote-shell 9/11` — `feature/remote-shell/derived-host-identity`
**Date**: 2026-11-16
**Passes**: 1

> Exploration 1 is the discovery shared by `#remote-shell` 9/11, 10/11 and 11/11, copied in full: the three
> nodes were planned from the same pass. Node-specific follow-up is appended as later Exploration sections.

## Combined Conclusions

The seven feature PRs of `#remote-shell` (#915–#921) merged on 2026-11-16 and the developer ran them
against a real Tddy Desktop with a real Coder deployment. Three things failed, none of which any
fixture reproduces, because every fixture leaves the daemon's two ids equal, lists one daemon, and
clones into a path that only a test host has.

1. **Two daemon ids, mixed.** With `daemon_instance_id_append_startup_timestamp: true` (the installed
   `desktop.yaml` has it) the daemon has a *routing* id per run (`LT-R1VXTH2V6H-<startup ms>`,
   `local_instance_id_for_config`) and a *durable* id (`LT-R1VXTH2V6H`,
   `local_base_instance_id_for_config`). The web addresses the daemon by the routing id everywhere: the
   common-room advertisement and `/api/config` both carry it, connections are keyed on it. The
   `ListDerivedHosts` handler stamps every derived host with the **durable** id as `parent_instance_id`
   (`derived_hosts.rs:47`). So the New-session form, which groups derived hosts by
   `d.parent === daemon.instanceId` (`CreateSessionHostField.tsx:43`), never matches, and the Hosts
   screen, whose rows carry durable ids from `ListKnownHosts`, resolves no connection for
   `useHostClient(HostService, durableId)` and asks nothing — neither rows nor a failure. The daemon log
   confirmed it: `ListKnownHosts` at 15:18:03 and 15:25:04 with no `ListDerivedHosts` after either.
2. **One listing for every daemon, kept only when all answer.** The web's derived directory source
   (`tddy-web/src/app/rpc/hostDirectory/derivedSource.tsx`) asks every connected daemon once, when the
   daemon set settles, and awaits all of them with `Promise.all`; a re-run discards the previous run's
   answer (`current = false`). With the flag off, the Hosts screen listed the Coder workspaces and the
   form still showed none: a second daemon in the room (`mantas-udoo-ros`) was in the set, and one
   daemon that never answers withholds every daemon's hosts, forever — nothing re-asks after an account
   is connected or the vault opens, and the form never shows the failure. Five calls hit the local
   daemon in the two seconds after start (the effect re-running as the room came up), none after.
3. **A fixture path in production.** `tddy-git/src/ssh_exec.rs:40 default_remote_repo_root` returns
   `/home/dev/<repo-name>` for every git URL; its doc comment says it is for "the fixture alias
   `buildbox` and example URLs". `setup_worktree_for_session_over_ssh` clones there and cuts
   `.worktrees/<session>` with `worktree add -b tddy-<session> HEAD`, ignoring the branch intent and base
   ref the local cut honours. An orchestrator start on `coder.remote-dev` failed with
   `mkdir: cannot create directory '/home/dev': Permission denied`. `remove_target_worktree` resolves the
   shell with `shell_for_target`, which refuses a Coder target (`NotACoderTarget`), and `rm -rf`s the
   worktree without `git worktree prune`.
4. **The orchestrator cannot name a target.** `INTERNAL_START_FIELDS` withholds `target` (#917's CI fix),
   `DescribeSessionStartOptions` lists only explicit SSH aliases, and `coder config-ssh` writes a
   wildcard `Host coder.*` that is not an alias. The agent fell back to `ssh_config_host`
   (agent local, tools remote) and the fixture path above. The TODO
   `docs/dev/todo/2026-11-16-orchestrator-start-tool-cannot-start-on-a-target.md` records the gap; the
   tool call carries the calling session's token, so the daemon can resolve the caller's vault for a
   Coder target.

Packages in play: `tddy-service` (proto), `tddy-host-service`, `tddy-client-web`, `tddy-web`,
`tddy-session-create-web`, `tddy-hosts-web`, `tddy-workspace-web` (accounts screen, vault prompt),
`tddy-git`, `tddy-agent-launch`, `tddy-session-activity`, `tddy-tool-engine`, `tddy-daemon-rpc`.

Constraints: the sweep (#924, `#remote-shell` 8/11) is restructuring `tddy-agent-launch`,
`tddy-session-lifecycle`, `tddy-accounts`, `tddy-worktree-service` and `tddy-daemon/mount_host.rs`
while these nodes are planned; a node touching `tddy-agent-launch` depends on the sweep's green.

## Exploration 1: the real deployment — 2026-11-16

**Agent**: parent Grep/Read over `origin/master` (`git show`, `git grep`) and the installed daemon's log
**Scope**: why a connected Coder account produced no selectable host, and why an orchestrator start
on a Coder workspace failed

### Sequence

1. Read `~/.tddy/logs/daemon` for `ListDerivedHosts`, `ConnectCoderAccount`, `coder`, `vault` — when
   the web asked, and what the daemon ran.
2. Read `packages/tddy-host-service/src/service/derived_hosts.rs` — the handler and the id it stamps.
3. Read `packages/tddy-daemon-kernel/src/daemon_identity.rs` — the two ids and the flag.
4. Grep `daemon_instance_id_append_startup_timestamp` in `~/.tddy/desktop.yaml` — `true`.
5. Read `packages/tddy-client-web/src/hostDirectory/derivedSource.ts`,
   `packages/tddy-web/src/app/rpc/hostDirectory/derivedSource.tsx`,
   `packages/tddy-web/src/app/rpc/selectedDaemon.tsx`, `packages/tddy-client-web/src/ports/clients.tsx`,
   `packages/tddy-client-web/src/connections/registry.ts`, `packages/tddy-livekit-web/src/connections/liveKit.tsx`,
   `packages/tddy-web/src/app/rpc/connections/localHost.ts` — how the form's daemon list is built and
   how a host id resolves to a connection.
6. Read `packages/tddy-hosts-web/src/DerivedHostRows.tsx`, `HostsScreen.tsx`, `useHostTooling.ts` —
   how the Hosts screen asks.
7. Read `packages/tddy-session-create-web/src/CreateSessionHostField.tsx`,
   `useCreateSessionPaneState.tsx` — how the form groups and submits a derived host.
8. Read `packages/tddy-daemon/src/daemon_config_service.rs:267` and
   `packages/tddy-daemon-livekit/src/livekit_peer_discovery.rs:686` — the id the web is handed.
9. Read `packages/tddy-tool-engine/src/start_session_args.rs`, `start_options.rs`,
   `packages/tddy-daemon-rpc/src/exec_tool/project_home_start_options.rs`,
   `tests/start_session_args_parity.rs` — what the orchestrator's start tool offers.
10. Read `packages/tddy-agent-launch/src/cli_session_worktree.rs`,
    `packages/tddy-git/src/ssh_worktree.rs`, `packages/tddy-git/src/ssh_exec.rs`,
    `packages/tddy-session-activity/src/remote_git_pack_execution.rs`,
    `packages/tddy-agent-launch/src/svc_signal_delete_session.rs` — where a remote checkout goes
    and how it is removed.
11. Read `packages/tddy-remote-shell/src/in_memory.rs` — the test double the contracts use.

### Grep / glob

| Tool | Pattern | Scope | Notable hits |
|------|---------|-------|--------------|
| grep | `ListDerivedHosts\|ListKnownHosts\|GetHostTooling` | `~/.tddy/logs/daemon` | derived listings only at 15:13:57–15:14:06 and 15:20:11–12 (page loads); `ListKnownHosts` 15:18:03, 15:25:04 with no derived listing after |
| grep | `daemon_instance_id_append_startup_timestamp` | `~/.tddy/desktop.yaml` | line 67: `true` |
| git grep | `local_base_instance_id_for_config` | `packages/tddy-host-service/src` | `service/derived_hosts.rs:47` (the parent id), `host_registry.rs:429` (the registry sighting) |
| git grep | `local_instance_id_for_config` | `packages/tddy-host-service/src/service` | `listings.rs:115` (`GetHostTooling` answers with it), `peer_routing.rs` (routing) |
| git grep | `remote_repo_path\|default_remote_repo_root` | `packages` | `tddy-git/src/ssh_worktree.rs`, `ssh_exec.rs:40`, `tddy-session-activity/src/remote_git_pack_execution.rs:75` |
| git grep | `INTERNAL_START_FIELDS` | `packages/tddy-tool-engine/src` | `start_session_args.rs:152` — `target` withheld |
| git grep | `ssh_config_hosts` | `packages/tddy-daemon-rpc/src` | `exec_tool/project_home_start_options.rs:105,256` — filled from `HostService::list_ssh_config_hosts` |

### Inspected files

#### `packages/tddy-host-service/src/service/derived_hosts.rs`

**Why**: which id a derived host names as its parent.
**Excerpt**:

```rust
let parent_instance_id = local_base_instance_id_for_config(&self.config);
let mut sources: Vec<Arc<dyn DerivedHostSource>> =
    vec![Arc::new(SshConfigDerivedHosts::new(
        Arc::clone(&self.host_user_files),
        parent_instance_id.clone(),
    ))];
sources.extend(self.derived_host_sources.iter().map(Arc::clone));
let coder_cli = self
    .cached_tooling(&parent_instance_id, &os_user, "")
    .await
    .map_err(Status::internal)?
    .coder_cli;
sources.push(Arc::new(self.coder_derived_hosts(&github_user, coder_cli)));
```

#### `packages/tddy-daemon-kernel/src/daemon_identity.rs`

**Why**: the two ids.
**Excerpt**:

```rust
/// - the **routing** id ([`local_instance_id_for_config`]) is what this process answers to in the
///   common room, made unique per *run* when `daemon_instance_id_append_startup_timestamp` is set;
/// - the **durable** id ([`local_base_instance_id_for_config`]) is what anything outliving the
///   process must key on — the host registry above all.
pub fn local_instance_id_for_config(config: &DaemonConfig) -> String {
    let base = local_base_instance_id_for_config(config);
    if config.daemon_instance_id_append_startup_timestamp {
        format!("{}-{}", base, process_startup_unix_ms_suffix())
    } else { base }
}
```

#### `packages/tddy-web/src/app/rpc/hostDirectory/derivedSource.tsx`

**Why**: when the form's derived hosts are fetched, and what survives a re-run.
**Excerpt**:

```ts
const parentIdsKey = directory.hosts.filter((host) => !host.parent).map((host) => host.hostId).join("\n");
useEffect(() => {
  const parents: DerivedHostParent[] = directory.hosts.filter((host) => !host.parent).map(/* connect(host.hostId) */);
  let current = true;
  void listDerivedHosts(parents, sessionToken ?? "").then((source) => { if (current) setDerived(source); });
  return () => { current = false; };
}, [parentIdsKey, connect, sessionToken]);
```

and in `tddy-client-web/src/hostDirectory/derivedSource.ts`:

```ts
const asked = parents.filter((parent) => parent.status === "connected");
if (asked.length === 0) return NO_DERIVED_HOSTS;
const answers = await Promise.all(asked.map(async (parent) => { /* listDerivedHosts({ sessionToken, daemonInstanceId: parentHostId }) */ }));
```

#### `packages/tddy-hosts-web/src/DerivedHostRows.tsx`

**Why**: how the Hosts screen asks a row's daemon.
**Excerpt**:

```tsx
const client = useHostClient(HostService, parentInstanceId);   // parentInstanceId = KnownHostEntry.instance_id (durable)
useEffect(() => {
  if (!parentOnline || !client) return;                          // no connection under the durable id → nothing asked, nothing shown
  client.listDerivedHosts({ sessionToken: sessionToken ?? "", daemonInstanceId: parentInstanceId }) …
```

#### `packages/tddy-client-web/src/ports/clients.tsx`

**Why**: how a host id becomes a connection.
**Excerpt**:

```ts
const resolved = parents.get(hostId) ?? hostId;   // a derived host → its parent; a daemon → itself
return (isReady?.(resolved) ?? true) ? registry.connectHost(resolved) : null;
```

`registry.connectHost` asks each provider; the IPC provider is registered under `/api/config`'s
`daemon_instance_id` and the LiveKit provider under participant identities — both routing ids.

#### `packages/tddy-session-create-web/src/CreateSessionHostField.tsx`

**Why**: how the form groups derived hosts.
**Excerpt**:

```tsx
const parents = daemons.filter((d) => !d.parent);
{parents.map((parent) => {
  const derived = daemons.filter((d) => d.parent === parent.instanceId);
  if (derived.length === 0) return null;
```

#### `packages/tddy-git/src/ssh_exec.rs` and `ssh_worktree.rs`

**Why**: where a remote checkout goes.
**Excerpt**:

```rust
/// Default checkout directory on an SSH target for the fixture alias `buildbox` and example URLs.
pub fn default_remote_repo_root(git_url: &str) -> String {
    let trimmed = git_url.trim();
    if trimmed.is_empty() || trimmed.contains("example.com") { "/home/dev/repo".to_string() }
    else if trimmed.starts_with("git@") { /* … */ format!("/home/dev/{name}") }
    /* … */
}
```

```rust
pub async fn setup_worktree_for_session_over_ssh(shell: &dyn RemoteShell, git_url: &str, session_id: &str) -> Result<String, String> {
    let remote_repo = default_remote_repo_root(git_url);
    let worktree_path = format!("{}/.worktrees/{}", remote_repo, session_id);
    /* clone if absent; fetch --all; */
    /* branch=tddy-{session_id}; git worktree add -b $branch {worktree} HEAD */
```

#### `packages/tddy-agent-launch/src/cli_session_worktree.rs`

**Why**: how the start reaches the remote cut, and what it drops.
**Excerpt**:

```rust
let worktree_base_ref = tddy_core::select_worktree_base_ref(start.selected_integration_base_ref, chain_base_ref);
let wt = if let Some(shell) = &start.shell {
    remote_worktree_through(start, git_url, shell.as_ref()).await?      // base ref, intent, new branch name: not passed
} else if start.ssh_alias.is_empty() {
    tddy_session_split::service_util::create_session_worktree(start.timeout, start.cut_label, repo_root, start.session_dir, worktree_base_ref).await?
} else {
    remote_worktree(start, git_url).await?
};
```

#### `packages/tddy-agent-launch/src/svc_signal_delete_session.rs`

**Why**: how a target's worktree is removed.
**Excerpt**:

```rust
pub(crate) async fn remove_target_worktree(target: &tddy_core::SessionTarget, worktree: &str) -> Result<(), Status> {
    let shell = crate::remote_agent_launch::shell_for_target(target)   // refuses a Coder target
        .map_err(|e| Status::invalid_argument(e.to_string()))?;
    shell.exec(ExecRequest::new(format!("rm -rf {}", shell_single_quote(worktree)))) …   // no `git worktree prune`
```

#### `packages/tddy-tool-engine/src/start_session_args.rs`

**Why**: what the orchestrator's start tool withholds.
**Excerpt**:

```rust
pub const INTERNAL_START_FIELDS: &[(&str, &str)] = &[
    /* … */
    ("target", "the session's own record names its target and the daemon places it there; not a form option"),
];
```

#### `packages/tddy-remote-shell/src/in_memory.rs`

**Why**: the double the new tests drive.
**Excerpt**: `InMemoryShell::new(name)`, `.answers(command, ExecOutput)`, `.answers_when(..)`,
`.hangs_on(command)`, `.is_unreachable(detail)`, `.has_file(..)`, `.streams(command)`, `.calls()`,
`.commands_run()`, `.file(path)`.

### Findings

- The derived-host feature was written against one id and the web runs on the other; only the
  timestamp flag separates them, and no fixture sets it.
- The form's listing is all-or-nothing across daemons and never refreshed; the Hosts screen's is
  per row and fresh, which is why they disagreed.
- Every SSH checkout, legacy or target, clones into `/home/dev/<repo>`; the remote cut ignores the
  branch intent and base ref; a Coder target's worktree cannot be removed on delete.
- The orchestrator has no way to name a derived host; the daemon can resolve one for it with the
  caller's own token.

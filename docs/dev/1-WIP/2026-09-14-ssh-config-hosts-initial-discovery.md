# Initial Discovery: list a host's SSH config Host aliases (`#ssh-exec` 1/4)

**Changeset**: [2026-09-14-ssh-config-hosts.md](./2026-09-14-ssh-config-hosts.md)
**Date**: 2026-09-14
**Passes**: 3

## Combined Conclusions

**This node (n1)** owns listing OpenSSH `Host` aliases from a daemon OS user's `~/.ssh/config`,
served as `host.HostService/ListSshConfigHosts`, and rendering them on the Hosts row as the
SSH connections cell the session dropdown in n2/n4 will reuse.

Nothing in the tree parses `~/.ssh/config`. `ListHostKeyCandidates` skips the file named
`config`. Hosts shows ssh-agent **keys**, not destinations. The list RPC must be addressed by
`daemon_instance_id` (same peer-forward as key listing) so a dropdown on codebase host B
reads B's config.

Honesty differs from key listing: an unreadable or unparseable config is a **failure**, not an
empty alias list. Empty means "this user has no Host aliases; LocalShell is the only choice".
Collapsing EPERM into empty would silently force local execution.

Parser rules (in-tree, no new crate): honor `Include`; skip `Host` stanzas whose patterns
contain wildcards (`*`, `?`); list each explicit alias once, stable order. Do not execute
`ssh -G` as the listing mechanism — that expands one host, it does not enumerate aliases.

n2/n3/n4 consume this list; they must not parse config themselves.

The rest of the stack context (split placement, tool-engine local FS, remote-git local pack
spawn, specialized-agent `replaces`) is in Exploration 1–2 and is **not** this node's to
implement.

## Exploration 1: interview-time architecture — 2026-09-14

**Agent**: Explore subagents (managed split, SSH UI, tddy-tools dispatch, specialized-agent routing, remote-git) plus parent reads of makers-lt `LocalShell`/`RemoteShell`
**Scope**: current split placement, tool transport, Hosts SSH UI, git forwarding, makers-lt analogue

### Sequence

1. Glob `**/*{LocalShell,RemoteShell}*` in `/Users/mantasi/Code/makers-lt` — user-cited analogue
2. Grep `LocalShell|RemoteShell` in makers-lt — find `shell.ts`, `remote-shell.ts`
3. Read `maker-build/maker-build/src/deployment/shell.ts` — `Shell` abstract + `LocalShell`
4. Read `maker-build/maker-build/src/deployment/remote-shell.ts` — `SSHConfig`, `RemoteHost`, `RemoteShell.withConnectedShell`
5. Explore tddy-coder for managed split / `codebase_daemon_instance_id`
6. Explore tddy-web host pickers and `~/.ssh/config`
7. Explore `tddy-tools` / `tddy-session-tool-client` / `tddy-tool-engine` dispatch
8. Explore specialized agents `replaces` vs exec catalog
9. Explore `tddy-remote-git-repo` / `RemoteGitService`

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Glob | `**/*{LocalShell,RemoteShell}*` | makers-lt | 0 filenames; classes live in `shell.ts` / `remote-shell.ts` |
| Grep | `LocalShell\|RemoteShell` | makers-lt | `shell.ts:57` `LocalShell extends Shell`; `remote-shell.ts:605` `RemoteShell` |
| Grep | `split.?repo\|managed split\|SplitRepo` | tddy-coder docs/packages | no literal; product language is managed codebase + split placement |
| Grep | `ssh.?config\|SshConfig\|~/.ssh` | tddy-coder | host-service key listing; `config` is not a key candidate |
| Grep | `specialized_agents\|SpecializedAgentDef` | packages | `session.proto` field 18; `agent_def.rs`; roster `replaces` |

### Inspected files

#### `makers-lt/.../shell.ts` / `remote-shell.ts`

**Why**: user asked to use LocalShell/RemoteShell as the concept.
**Excerpt**:

```ts
export abstract class Shell {
  abstract execCommand(command: string, options?: { workdir?: string }): Promise<string>;
  abstract putFileContent(path: string, content: string): Promise<void>;
  abstract getFileContent(path: string): Promise<string>;
}
export class LocalShell extends Shell { /* child_process */ }
export interface SSHConfig { host: string; user: string; port?: number; privateKeyPath?: string; }
export class RemoteHost extends Shell {
  private buildSSHCommand(command: string): string {
    return `ssh ${optionsStr}-p ${this.sshPort} ${this.sshTarget} ${this.embedNestedCommand(command)}`;
  }
}
```

#### `packages/tddy-session-tool-client/src/lib.rs`

**Why**: where a new SSH transport would appear — and why it should not.
**Finding**: transport priority is sandbox IPC > LiveKit > HTTP. Split sessions already reach B
over LiveKit. Putting OpenSSH in `tddy-tools` fights the jail. SSH belongs in the daemon-side
engine after `ExecuteTool` arrives.

#### `packages/tddy-tool-engine/src/lib.rs`

**Why**: current exec implementation.
**Excerpt**:

```rust
pub async fn execute_tool(
    worktree_root: &Path,
    tool_name: &str,
    args_json: &str,
    registry: &TaskRegistry,
    session_id: &str,
) -> ToolOutcome {
    execute_tool_with_env(worktree_root, tool_name, args_json, registry, session_id, &[]).await
}
```

`contain_path` requires a local canonicalizable `worktree_root`. RemoteShell must keep the same
containment contract against the **remote** worktree path.

#### `docs/ft/daemon/remote-managed-worktree.md` / `CreateSessionPane.tsx`

**Why**: existing split UX and start path.
**Finding**: `daemon_instance_id` = agent host; `codebase_daemon_instance_id` = worktree daemon.
`canChooseCodebaseHost` gates `create-session-codebase-host-select`. Tools on split: LiveKit
`StreamExecuteTool` to B. B has no SSH target today.

#### `packages/tddy-core/src/session_metadata.rs`

**Why**: where `ssh_config_host` must persist.
**Excerpt**: fields `codebase_daemon_instance_id`, `codebase_session_id`,
`agent_daemon_instance_id`, `agent_session_id`. `deny_unknown_fields` — a new key is additive
with `skip_serializing_if`.

#### `packages/tddy-worktree-service/src/remote_git_service.rs`

**Why**: n3 Serve-over-SSH seam.
**Excerpt**: `GitVerb::{UploadPack, ReceivePack}` maps to `git upload-pack` / `git receive-pack`
spawned as the project OS user on local `main_repo_path`. Client `tddy-remote-git-repo` is
unchanged if Serve still speaks the same proto.

#### `packages/tddy-host-service/src/host_private_key.rs`

**Why**: listing honesty for n1.
**Excerpt**: `list_key_candidates` returns a `Vec`, never a failure; missing `~/.ssh` and
unreadable `~/.ssh` are indistinguishable. `config` is not a key candidate. n1 must **not**
collapse a failed config read into an empty alias list: empty means "no SSH targets, use
LocalShell", which is a real operator choice.

### Findings

- makers-lt: one `Shell` interface, local vs `ssh` CLI. tddy should copy the shape, not the TS.
- Split already exists and stays; SSH is an execution override on the code-managing host.
- No `~/.ssh/config` parser in the tree. n1 owns it.
- Specialized-agent withdrawal already happens before exec dispatch; n2 must not re-route
  replaced names. n3 keeps AC37 clones working when the project tree is on T.

## Exploration 2: backlog + proto/handler seams — 2026-09-14

**Agent**: parent Grep/Glob/Read
**Scope**: `docs/dev/todo/`, `host.proto`, `session.proto`, worktree setup, Hosts tooling UI

### Sequence

1. `ls docs/dev/todo | sort -r | head -40`
2. Grep todo dir for ssh / split / remote-git / managed codebase / exec tool
3. Read `2026-09-06-ssh-agent-bounds-stop-at-the-conversation.md`
4. Read `2026-09-06-loading-a-key-needs-a-privileged-socket-path.md`
5. Read `2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md`
6. Read `2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md`
7. Read `2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md`
8. Read `packages/tddy-service/proto/host.proto` `HostService` + `ListHostKeyCandidates*`
9. Read `packages/tddy-core/src/worktree.rs` `setup_worktree_for_session`
10. Read `packages/tddy-web/src/components/hosts/HostRowTooling.tsx`
11. Read `docs/ft/web/hosts-screen.md`, `docs/ft/web/hosts-screen-tooling.md`

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Glob | `docs/dev/todo/*.md` | todo | ssh-agent bounds, privileged socket, unprivileged probe, remote-git gaps, split room restart |
| Grep | `ListHostKeyCandidates` | `host.proto` | rpc at line 68; request `session_token` + `daemon_instance_id` |
| Grep | `fn setup_worktree` | packages | `tddy-core/src/worktree.rs` local `repo_root` only |
| Grep | `create-session-codebase-host-select` | CreateSessionPane.tsx | line 1143 |

### Inspected files

#### `packages/tddy-service/proto/host.proto`

**Why**: n1 RPC neighbour.
**Excerpt**: `ListHostKeyCandidates` is addressed by `daemon_instance_id` so listing runs on the
host the operator is looking at. n1 `ListSshConfigHosts` must honour the same addressing (peer
forward), or the session dropdown on a remote codebase host would list A's aliases.

#### `packages/tddy-core/src/worktree.rs`

**Why**: n2 materialize-over-SSH must reuse this contract, not a second worktree story.
**Excerpt**: `setup_worktree_for_session(repo_root, session_dir)` fetches the integration base
and `git worktree add` under `.worktrees/`. RemoteShell runs the same git operations on T.

#### Step 2b verdicts (carried into per-node `## Prerequisites`)

| Entry | Verdict | Why |
|-------|---------|-----|
| [2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md) | ⚠ DURING n1 | Config parse runs as the daemon's OS user on supervised installs, same as tooling probes. Do not fabricate an empty Host list on EPERM. |
| [2026-09-06-loading-a-key-needs-a-privileged-socket-path.md](../todo/2026-09-06-loading-a-key-needs-a-privileged-socket-path.md) | ⚠ DURING n2 | RemoteShell needs the ssh-agent the Hosts row already shows. Supervised daemon may only see its own agent. Record, do not absorb supervisor fd-passing. |
| [2026-09-06-ssh-agent-bounds-stop-at-the-conversation.md](../todo/2026-09-06-ssh-agent-bounds-stop-at-the-conversation.md) | — Unrelated | Agent wire bounds; this stack shells out to `ssh`, it does not speak the agent protocol. |
| [2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md](../todo/2026-08-15-remote-git-repo-over-livekit-deliberate-gaps.md) | ⚠ DURING n3 | No peer forwarding, no v2, global stream cap. n3 must not invent a second git protocol; it only changes where Serve execs pack verbs. |
| [2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md](../todo/2026-08-14-a-split-session-s-room-is-not-re-opened-when-the-daemon-restarts.md) | ⚠ DURING n4 | Split+SSH still needs the session room on A. Do not absorb the restart sweep into n4. |
| [2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md](../todo/2026-08-13-cursor-cli-cannot-enforce-managed-codebase-mode.md) | — Unrelated | cursor-cli split remains refused; SSH is claude-cli / managed. |
| [2026-08-13-tddy-coder-remote-never-completes-a-session-bootstrap.md](../todo/2026-08-13-tddy-coder-remote-never-completes-a-session-bootstrap.md) | — Unrelated | CLI `--remote`; this stack is daemon/UI. |

### Findings

n1 RPC should mirror `ListHostKeyCandidates` addressing but **not** its empty-on-error honesty:
an unreadable config is `FAILED` / a distinguished error, never "no Host aliases". n2 worktree
setup is the existing `setup_worktree_for_session*` contract executed through RemoteShell. n3
does not change `tddy-remote-git-repo`. n4 only retargets the dropdown and forwards the field.

## Exploration 3: n1 HostService listing seam — 2026-09-14

**Agent**: parent Read
**Scope**: `HostServiceImpl::list_host_key_candidates`, Hosts ssh-agent Cypress, `host-service.md`

### Sequence

1. Read `packages/tddy-host-service/src/service.rs` around `ListHostKeyCandidates` (line 729)
2. Read `packages/tddy-web/cypress/component/HostsScreenSshAgentAcceptance.cy.tsx` header
3. Read `packages/tddy-host-service/docs/host-service.md` Methods table

### Inspected files

#### `packages/tddy-host-service/src/service.rs`

**Why**: n1 handler pattern.
**Excerpt**: the method peer-forwards on `daemon_instance_id`, resolves `os_user`, then calls
`list_key_candidates`. n1's `list_ssh_config_hosts` follows the same forward + os_user, then
parses `~/.ssh/config` (and `Include` paths) as that user.

#### `packages/tddy-web/cypress/component/HostsScreenSshAgentAcceptance.cy.tsx`

**Why**: acceptance-test shape for the Hosts SSH connections cell.
**Finding**: `mountWithRpc` + `anInMemoryRpcBackend`; four ssh-agent states asserted as
mutually distinguishable in one test. n1's Hosts cell needs the same: aliases listed, empty
config, unreadable config, and unsupported platform must not collapse.

### Findings

Add `ListSshConfigHosts` next to `ListHostKeyCandidates` on `HostService`. Do not hang the
alias list off `GetHostTooling`: the session dropdown needs a cheap unary list, not the full
tooling probe. Hosts row still grows an SSH connections cell that *calls* the new RPC (or is
fed by the page the way ssh-agent is fed from tooling — either is fine if the session form
uses the list RPC directly).


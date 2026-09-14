# Initial Discovery: run session exec tools over an SSH Host alias (`#ssh-exec` 2/4)

**Changeset**: [2026-09-14-ssh-exec.md](./2026-09-14-ssh-exec.md)
**Date**: 2026-09-14
**Passes**: 3

## Combined Conclusions

**This node (n2)** owns `LocalShell`/`RemoteShell` on the code-managing daemon, session
`ssh_config_host`, co-located StartSession + create-session dropdown (Host A's list from n1's
RPC), worktree materialize over SSH, and routing every exec-catalog tool through RemoteShell
when an alias is set.

`execute_tool` today takes a local `Path` `worktree_root` and `contain_path` canonicalizes on
the host FS. SSH from `tddy-tools` inside a jail is the wrong seam. Dispatch stays IPC/HTTP/
LiveKit; the daemon's engine opens `ssh <alias>`. Empty alias = LocalShell (today). Native
agent FS tools stay off when an alias is set (managed-codebase semantics).

n1 owns the list RPC — this node consumes it (UI doubles it in tests). n3 owns RemoteGit
Serve-over-SSH. n4 owns split filtering to codebase host B. Specialized-agent `replaces`
already withdraws before dispatch; this node must not re-route those names.

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

## Exploration 3: n2 engine and session start seams — 2026-09-14

**Agent**: parent Read
**Scope**: `execute_tool`, `SessionMetadata`, `setup_worktree_for_session`, CreateSessionPane host select

### Sequence

1. Read `packages/tddy-tool-engine/src/lib.rs` `execute_tool` / `contain_path`
2. Read `packages/tddy-core/src/session_metadata.rs` split fields
3. Read `packages/tddy-core/src/worktree.rs` `setup_worktree_for_session`
4. Grep `createSessionHostSelect` in testIds.ts

### Findings

`execute_tool` must grow a shell backend (local vs `ssh -o BatchMode=yes <alias> --`) without
changing the MCP dispatch stack. Session metadata needs `ssh_config_host` plus the remote
worktree path the engine contains against. Worktree setup is the existing git worktree
contract executed through RemoteShell on T (ensure clone, then `git worktree add`).
Create-session dropdown `create-session-ssh-config-select` reads `ListSshConfigHosts` for the
**session host** (A) in this node; n4 retargets it.


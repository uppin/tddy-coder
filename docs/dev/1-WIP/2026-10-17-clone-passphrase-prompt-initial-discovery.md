# Initial Discovery: Clone passphrase prompt (ssh-askpass bridge)

**Changeset**: [2026-10-17-clone-passphrase-prompt.md](./2026-10-17-clone-passphrase-prompt.md)
**Date**: 2026-10-17
**Passes**: 2

## Combined Conclusions

**The trigger.** Creating a remotely managed codebase on the `udoo` daemon from a locally installed Tddy
Desktop failed three different ways in turn, each one peeling back the next layer:

1. `Host key verification failed` — the clone's OS user had no `github.com` in `known_hosts`
   (operator fix, not a code defect).
2. `deadline_exceeded: ensure_project_available_for_start: clone timed out` — with `os_user=mantas` the
   host key was trusted, GitHub **accepted** `~/.ssh/id_rsa`, but the key is passphrase-protected.
   ssh could not sign (`we did not send a packet, disable method` under `BatchMode`), and the daemon's
   clone — no `BatchMode`, `stdin` null, no tty, no agent — sat waiting for a passphrase nobody could
   type until the 300 s deadline.
3. The same log shows the desktop gave up at **30.0 s** (`DeleteSession` 30 s after `StartSession`):
   that is `PEER_FORWARD_TIMEOUT` on the forwarded `StartSession`, not the 300 s clone deadline.

**What already exists (State A is mostly built — only the clone is not wired to it).**
`#hosts-screen` shipped the whole passphrase-to-agent loop for a *different* trigger (an operator
pressing "add key" on the Hosts screen): `HostService.StreamHostPrompts` + `AnswerHostPrompt`
(RSA-OAEP answer under the host's public key), a `HostPromptRegistry` (`issue` / `awaited_answer` /
120 s TTL / single use / per-GitHub-user), `HostPassphraseDialog` on the web side, and
`AddHostKey`, which decrypts the answer **in the daemon**, unlocks the key with the `ssh-key` crate
and hands the identity to the agent over the ssh-agent wire protocol (`WireProtocolAgentKeyAdder`),
the socket found by `WellKnownAgentSockets`.

**What is missing, precisely.**

| Gap | Where |
|---|---|
| Nothing connects a clone to a prompt. The clone runs plain `git clone` with no `SSH_ASKPASS`, no `SSH_AUTH_SOCK`, no `BatchMode`/`ConnectTimeout`. | `packages/tddy-spawn/src/spawner.rs:620` (`clone_as_user_with_env`) |
| The forked-worker clone has **no env channel** (`CloneRequest` is `{os_user, git_url, destination}`), so env can only reach the clone via the in-process fallback today. | `packages/tddy-spawn/src/spawn_worker.rs`, `svc_ensure_project_available_for_start.rs:62-80` |
| Under a supervisor, any env key not in `spawn_policy.allowed_env_keys` is refused. | `packages/tddy-spawn/src/supervisor_spawn.rs:109-113` |
| No askpass helper binary exists; the daemon has no loopback endpoint a helper could ask. | — |
| The prompt dialog is mounted only by `HostAddKeyAction` and `HostDesktopOverlay`, both on the Hosts screen. A start from `CreateSessionPane` mounts nothing. | `packages/tddy-web/src/components/hosts/` |
| A forwarded `StartSession` has a 30 s deadline; a prompt lives 120 s. | `packages/tddy-daemon-kernel/src/peer_forwarding.rs:95`, `host_prompts.rs:28` |
| `ensure_project_available_for_start` receives `os_user` and `session_token` but not the GitHub user a prompt must be `issued_for`. | `svc_ensure_project_available_for_start.rs:119-127` |

**Decisions taken in the interview (2026-10-17).**

- **Capture mechanism: `SSH_ASKPASS` helper.** The clone runs with `SSH_ASKPASS=<tddy helper>`,
  `SSH_ASKPASS_REQUIRE=force`, `-o AddKeysToAgent=yes`. ssh itself asks the helper; the helper asks the
  daemon; the daemon raises the existing `HostPrompt`. **Not** `BatchMode` (it disables askpass).
- **Agent: the OS user's existing agent**, via `SSH_AUTH_SOCK` from `WellKnownAgentSockets`. No agent
  reachable ⇒ report it, never start one.
- **Scope: the auto-clone only**, plus fail-fast hardening (`ConnectTimeout`, `GIT_TERMINAL_PROMPT=0`).
  Other daemon git-over-SSH calls are recorded as a TODO.
- **Deadline: a clone-bearing `StartSession` is forwarded with prompt-TTL + clone time (the existing
  `spawn_worker_request_timeout`), not 30 s.**

**Why the askpass design dissolves the two open "privileged socket" TODOs for this path.**
`AddHostKey` writes to the agent from the *daemon's* process, which is why
`docs/dev/todo/2026-09-06-loading-a-key-needs-a-privileged-socket-path.md` says a supervised
(unprivileged) daemon cannot add to another user's agent. With askpass, `ssh` — already running **as the
OS user** — performs `AddKeysToAgent`, so the daemon never opens the user's agent socket. It only has to
*name* the socket in `SSH_AUTH_SOCK` (the resolver returns the path even when it cannot `stat` it).

**Constraints and risks to plan around.**

- **Plaintext crosses one local hop.** The daemon decrypts the answer; the helper must receive it to
  print to ssh. That hop is loopback HTTP + a per-clone token, the pattern `tddy-remote-git-repo`
  already uses (`--daemon-url` / `--session-token`, `daemon_rpc.rs`). The plaintext must never be logged
  and the token must die with the clone.
- **ssh invokes askpass for more than passphrases** (host-key `yes/no`, password auth). The helper must
  classify by prompt text: only `Enter passphrase for key '<path>':` maps to
  `PromptKind::SshKeyPassphrase`; anything else exits non-zero so ssh fails fast.
- **`SSH_ASKPASS_REQUIRE=force` needs OpenSSH ≥ 8.4**; otherwise askpass runs only when `DISPLAY` is set.
- **Two backends**: forked worker (needs the env channel added) and `tddy-supervisor` (needs
  `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE`, `SSH_AUTH_SOCK`, `GIT_SSH_COMMAND`/`GIT_TERMINAL_PROMPT` and the
  helper's token env in `allowed_env_keys`; the helper binary must resolve as a sibling of the daemon
  exactly like `tddy-remote-git-repo`, which means `install`, `release`, `publish.sh`,
  `scripts/dev-runtime-binaries.sh` and `tddy-e2e/tests/dev_runtime_binaries.rs` all list it).
- **A timed-out clone leaves a partial destination** and the clone is skipped next time
  (`test -e "$1"`); and the abandoned blocking task keeps running. Fail-fast addresses the cause; cleanup
  of a failed clone's destination is a related defect to decide on.

**Packages in play.** `tddy-spawn`, `tddy-agent-launch`, `tddy-host-service`, `tddy-service` (proto),
`tddy-daemon-kernel` (forward deadline), `tddy-web`, a new helper package, plus `install`/`release`/
`publish.sh`/`scripts/dev-runtime-binaries.sh`.

**Verified empirically (Exploration 2).** With real OpenSSH 10.3 on macOS, no tty, a user-mode `sshd`, a
passphrase-protected ed25519 key and a fresh `ssh-agent`: ssh called `SSH_ASKPASS` exactly once with
`Enter passphrase for key '<path>': `, the clone completed, `ssh-add -l` then listed the key, and a second
clone with the key in the agent never called the helper. So AC1–AC3's mechanism is sound. **ssh truncates
the path inside the prompt to 100 characters** (`%.100s`), so the helper must treat it as display text and
never open it as a file.

**Open questions for the PRD.**
1. Where does the helper's loopback endpoint live — a new `AskpassService` on the daemon's existing
   Connect-HTTP listener, or a unix socket the daemon creates? (Unprivileged daemon cannot chown a
   socket to another OS user, which favours loopback HTTP.)
2. How does the web UI learn *which host's* prompt stream to subscribe to during a start — the target
   `daemon_instance_id` of the `StartSession` it issued.
3. Does a failed clone remove its half-written destination?

## Exploration 1: Clone path, prompt machinery and deployment surface — 2026-10-17

**Agent**: parent Grep/Glob/Read
**Scope**: how a remote managed codebase is cloned; what the Hosts screen already does with passphrases;
what a helper binary and a deadline change would touch.

### Sequence

1. Grep `git clone failed` in `packages/` — locate the error's origin.
2. Read `packages/tddy-spawn/src/spawner.rs:600-735` — the clone as the OS user.
3. Read `packages/tddy-spawn/src/supervisor_spawn.rs:95-160, 280-370` — the supervised clone and its env.
4. Read `packages/tddy-projects/src/project_provision.rs:60-135` — the `clone {url} into {dest}: ` prefix.
5. Read `packages/tddy-agent-launch/src/svc_ensure_project_available_for_start.rs:25-230` — which
   backend clones, what env it carries, where the 300 s timeout fires.
6. Grep `ssh-agent|SSH_AUTH_SOCK|SSH_ASKPASS|passphrase` in `packages/` — find prior art.
7. Read `packages/tddy-service/proto/host.proto:25-70, 135-160, 235-300` — prompt RPCs and events.
8. Read `packages/tddy-host-service/src/host_prompt_stream.rs`, `host_prompts.rs` (trait, `PendingPrompt`,
   `PROMPT_TTL`), `ssh_agent_add.rs`, `ssh_agent.rs:425-500`, `service.rs:600-760` (`add_host_key`).
9. Grep `StreamHostPrompts|HostPassphraseDialog|useHostPrompts(` in `packages/tddy-web/src` — who mounts
   the dialog.
10. Read `packages/tddy-remote-git-repo/Cargo.toml`; grep `tddy-remote-git-repo` in `install`, `release`,
    `publish.sh`; read `scripts/dev-runtime-binaries.sh` — what shipping a helper binary touches.
11. Grep `PEER_FORWARD_TIMEOUT` and `PROMPT_TTL`; read `config.rs` `spawn_worker_request_timeout`.
12. Step 2b: `ls`/`grep` over `docs/dev/todo/` and `packages/*/docs/code-issues/` for the touched files.

### Grep / glob

| Tool | Pattern / glob | Path scope | Notable hits |
|------|----------------|------------|--------------|
| Grep | `git clone failed` | `packages/**/*.rs` | `spawner.rs:731`, `supervisor_spawn.rs:353,357` |
| Grep | `clone_as_user\|clone_repo_via_supervisor` | `packages/**/*.rs` | `svc_ensure_project_available_for_start.rs:49,79`, `coordinate_handlers.rs:127,145,304,322`, `spawn_worker.rs:408` |
| Grep | `ssh-agent\|SSH_AUTH_SOCK\|SSH_ASKPASS\|passphrase` | `packages` | `HostPassphraseDialog.tsx`, `ssh_agent*.rs`, `host.proto` |
| Grep | `WellKnownAgentSockets` | `packages/**/*.rs` | `ssh_agent_add.rs:65`, `host_tooling.rs:460` |
| Grep | `StreamHostPrompts\|HostPassphraseDialog\|useHostPrompts(` | `packages/tddy-web/src` | `HostAddKeyAction.tsx:155,397`, `HostDesktopOverlay.tsx:175,243` only |
| Grep | `PEER_FORWARD_TIMEOUT` | `packages` | `peer_forwarding.rs:95` = 30 s |
| Grep | `PROMPT_TTL` | `host_prompts.rs` | `:28` = 120 s |
| Grep | `tddy-remote-git-repo` | `install release publish.sh` | six lists that ship it |

### Inspected files

#### `packages/tddy-spawn/src/spawner.rs`

**Why**: the clone the user's log shows (`clone_as_user: will run git clone as os_user=…`).
**Excerpt**:

```rust
let mut cmd = std::process::Command::new("sh");
cmd.arg("-c")
    .arg(r#"if test -e "$1"; then exit 0; fi; exec git clone "$2" "$1""#)
    .arg("sh").arg(dest_str).arg(git_url)
    .env("HOME", &home_dir)
    .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
// Transport-shim env vars (e.g. GIT_SSH_COMMAND + TDDY_* for a facilitator clone).
for (key, value) in extra_env { cmd.env(key, value); }
```

`cmd.output()` blocks until `git` exits — there is no deadline of its own; the 300 s bound is the
*caller's* `tokio::time::timeout`, which abandons the blocking task without killing the child.

#### `packages/tddy-agent-launch/src/svc_ensure_project_available_for_start.rs`

**Why**: which backend runs the clone and where the timeout is.
**Excerpt**:

```rust
ForkedWorker => {
    if let Some(ref client) = spawn_client {
        if ssh_command.is_none() {
            // No transport env var to carry: the forked worker's `clone_repo` is the original path
            return client.clone_repo(spawn_worker::CloneRequest { os_user, git_url, destination })…
        }
    }
    spawner::clone_as_user_with_env(&os_user_owned, git_url, dest, &extra)…
}
…
match tokio::time::timeout(timeout, handle).await {
    Err(_elapsed) => Err(Status::deadline_exceeded("ensure_project_available_for_start: clone timed out")),
```

#### `packages/tddy-host-service/src/service.rs` (`add_host_key`)

**Why**: the prompt → decrypt → agent flow that must be reused, not reimplemented.
**Excerpt**:

```rust
let prompt = self.host_prompts.issue(&github_user, PromptKind::SshKeyPassphrase, &req.subject, now);
let waiting = self.host_prompts.awaited_answer(&prompt.prompt_id);
let answer = match waiting { Some(handoff) => answer_before_expiry(handoff, &prompt).await, None => None };
…
// Everything that follows blocks — an RSA decrypt, a bcrypt-pbkdf unlock and a socket conversation
// with the agent — so it runs on the blocking pool …
```

The `issue` → `awaited_answer` → `answer_before_expiry` → `keypair.decrypt` sequence is the reusable
core; the part specific to `AddHostKey` is `unlock_and_add`, which the askpass flow does **not** want
(ssh unlocks and adds the key itself).

#### `packages/tddy-host-service/src/ssh_agent.rs:444-500` (`WellKnownAgentSockets`)

**Why**: how the clone is pointed at the user's agent.
**Excerpt**:

```rust
Ok(our_own_agent_socket(uid).or_else(|| runtime_dir_agent_socket(uid)))
// candidates: /run/user/<uid>/ssh-agent.socket, gnupg/S.gpg-agent.ssh, keyring/ssh
// PermissionDenied on stat → path handed back anyway so connect() reports the truth
```

#### `packages/tddy-daemon-kernel/src/peer_forwarding.rs:95` and `host_prompts.rs:28`

```rust
pub const PEER_FORWARD_TIMEOUT: Duration = Duration::from_secs(30);
pub const PROMPT_TTL: Duration = Duration::from_secs(120);
```

### Findings

- The passphrase-to-agent machinery is built; the clone is the one consumer not wired to it.
- Askpass beats `AddHostKey`-then-retry here because ssh adds the key to the agent as the OS user,
  sidestepping the privileged-socket TODO, and because the clone proceeds in the same attempt.
- The user's 30 s abandonment is the peer-forward deadline, an independent obstacle that a correct
  askpass flow would still hit.
- Step 2b: no ⛔ blocking items and no 🚧 claimed items. ⚠ constraints are recorded in the changeset's
  `## Prerequisites`.

## Exploration 2: Does real ssh drive askpass the way the design assumes? — 2026-10-17

**Agent**: parent Bash
**Scope**: de-risk the central premise before pinning it in acceptance tests.

### Sequence

1. `ssh-keygen` a host key and a passphrase-protected (`correct horse`) ed25519 user key in the scratchpad.
2. Seed a bare repo; start `/usr/sbin/sshd -D -e -f sshd_config` as the current user on an ephemeral
   loopback port (`UsePAM no`, `StrictModes no`, `PasswordAuthentication no`).
3. First attempt used `setsid` — **absent on macOS**; re-ran without it (`SSH_ASKPASS_REQUIRE=force` makes
   ssh skip the tty regardless).
4. `ssh-agent -s`; `git clone ssh://…` under `env -i` with `SSH_ASKPASS=<script>`,
   `SSH_ASKPASS_REQUIRE=force`, `GIT_TERMINAL_PROMPT=0`, `SSH_AUTH_SOCK`, and
   `GIT_SSH_COMMAND="ssh -F /dev/null -i <key> -o IdentitiesOnly=yes -o AddKeysToAgent=yes -o ConnectTimeout=5 …"`.
5. Repeated the clone with the key now in the agent.

### Findings

| Observation | Consequence |
|---|---|
| askpass called once, with `Enter passphrase for key '<path>': ` | the prompt-text pattern the helper classifies on is confirmed |
| the path in that prompt was cut at 100 characters | subject is display text; tests must cover a long path |
| `ssh-add -l` listed the key afterwards | `AddKeysToAgent=yes` is enough; the daemon never touches the agent |
| second clone: helper not called | AC3 holds |
| no `setsid` on macOS | tests must not rely on dropping the controlling tty; `SSH_ASKPASS_REQUIRE=force` is what makes this work |

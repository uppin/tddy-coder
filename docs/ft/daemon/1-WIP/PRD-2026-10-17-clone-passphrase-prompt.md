# Clone passphrase prompt - PRD

**Date**: 2026-10-17
**PRD Type**: Enhancement (with a Bug Fix component: the clone hangs instead of failing)

## Affected Features

- **Primary Feature**: [Remote codebase mode](../remote-codebase-mode.md) — a remotely managed codebase is
  cloned on the owning daemon over SSH as the mapped OS user; this PRD makes that clone able to ask the
  operator for a key passphrase and fail fast on everything else.
- **Related Feature 1**: [Hosts screen — add key](../../web/hosts-screen-add-key.md) — owns the host
  prompt channel (`StreamHostPrompts` / `AnswerHostPrompt`), the host keypair encryption and the
  passphrase dialog this PRD reuses for a second trigger.
- **Related Feature 2**: [Project concept](../project-concept.md) — auto-provisioning a project's working
  copy before a session starts.
- **Related Feature 3**: [Remote git repo](../remote-git-repo.md) — the `tddy-remote-git-repo` shim is the
  pattern (a helper binary a clone's `git` execs, talking back to the daemon) the new helper follows.
- **Related Feature 4**: [Supervisor](../../supervisor/) — brokered clones must carry the new environment
  through `spawn_policy.allowed_env_keys`.

## Summary

When the daemon clones a project over SSH as an OS user whose key is passphrase-protected, the clone
currently **hangs for five minutes and then fails with `clone timed out`**, with nothing telling the
operator why. After this change the daemon captures ssh's passphrase request, shows it to the operator in
the UI, hands the answer to ssh, and ssh loads the unlocked key into the OS user's existing ssh-agent so
the clone — and later git operations by that user — proceed without asking again.

Every other reason an SSH clone cannot proceed (untrusted host, no usable key, unreachable network,
no agent) becomes a prompt, readable failure within seconds rather than a silent wait.

## Background

Creating a remote managed codebase for `tddy-coder` on the `udoo` daemon from a local Tddy Desktop failed
in three stages (see the initial discovery for the log lines):

1. `Host key verification failed` — fixed on the host by trusting `github.com`.
2. `deadline_exceeded: … clone timed out` — GitHub accepted the user's `~/.ssh/id_rsa`, but the key is
   passphrase-protected. ssh asks for the passphrase on a tty or through `SSH_ASKPASS`; the daemon's
   clone has neither, so it waited for input that could never arrive.
3. The desktop abandoned the start after **30 s** — the peer-forward deadline — regardless.

The Hosts screen already knows how to ask an operator for a passphrase safely: the question travels on
`StreamHostPrompts`, the answer returns encrypted under the host's public key on `AnswerHostPrompt`, and
only the daemon ever decrypts it. That flow is wired to one trigger (pressing "add key" on a host row).
The clone is the trigger that needs it most and does not have it.

## Proposed Changes

### What's Changing

1. **A passphrase request during a clone becomes an operator prompt.** The clone runs ssh with an askpass
   helper. When ssh asks for a key's passphrase, the helper asks the daemon, the daemon raises a
   `HOST_PROMPT_KIND_SSH_KEY_PASSPHRASE` prompt (subject: the key path ssh named) for the operator who
   started the session, waits for the encrypted answer, decrypts it in the daemon and returns it to the
   helper, which gives it to ssh.
2. **ssh loads the key into the agent.** The clone passes `AddKeysToAgent=yes` and `SSH_AUTH_SOCK` naming
   the OS user's existing agent socket. ssh — running as that OS user — adds the unlocked key itself, so
   a supervised, unprivileged daemon never has to open another user's agent socket. When no agent is
   reachable the clone fails saying so; the daemon never starts an agent.
3. **Only passphrase requests are answered.** The helper recognises ssh's
   `Enter passphrase for key '<path>':` prompt. Any other askpass request (host-key confirmation,
   password) is refused so ssh fails immediately with its own message.
4. **Clones fail fast.** The clone sets `ConnectTimeout` and `GIT_TERMINAL_PROMPT=0`, so an unreachable
   host or an unanswerable credential becomes an error naming the cause within seconds. A prompt that is
   not answered within its 120 s lifetime fails the clone with "nobody answered the passphrase prompt".
5. **The web UI shows the prompt wherever a start is in flight.** The passphrase dialog, today mounted
   only from the Hosts screen, also appears over the session-creation form while a start that must clone
   is waiting, subscribed to the prompt stream of the host that is cloning.
6. **A forwarded `StartSession` waits as long as the peer may legitimately take.** The plain
   `StartSession` forward uses the 30 s peer-forward deadline, while split sessions and agent clones
   already forward with `split_forward_deadline` (the peer's `spawn_worker_request_timeout` plus one
   forward deadline, 330 s by default) because the peer may clone first. The plain forward adopts that
   same budget. The caller cannot know whether the peer will clone, so the budget applies to every
   forwarded start; the cost is that a peer whose RPC participant is gone is reported after that wait
   instead of after 30 s (the trade-off `split_forward_deadline` already documents and accepts).
7. **Both clone backends carry the environment.** The forked-worker clone gains an environment channel
   (today only the in-process fallback has one); the supervised clone's `allowed_env_keys` documents and
   admits the new keys.
8. **A new helper binary ships beside the daemon**, resolved as its sibling the way
   `tddy-remote-git-repo` is, and listed wherever that binary is listed.

### What's Staying the Same

- `AddHostKey` and the Hosts screen add-key flow, the prompt wire format, the 120 s prompt lifetime,
  single-use answers and per-GitHub-user prompt scoping are unchanged.
- The passphrase is never logged, never stored, and never sent anywhere in plaintext except one hop to the
  helper process on the same host (loopback, per-clone token, same OS user).
- Facilitator (`tddy-remote-git-repo`) clones are unchanged and take precedence: they do not use SSH
  keys on the owning daemon.
- Other daemon git-over-SSH operations (fetch, push, worktree sync) are **out of scope**; they keep their
  current behaviour and are recorded in `docs/dev/todo/`.
- No daemon-owned ssh-agent is introduced.

## Impact Analysis

### Technical Impact

- **`tddy-spawn`**: `clone_as_user_with_env` (and the worker `CloneRequest`) carry environment;
  supervised clone documents the added `allowed_env_keys`.
- **`tddy-agent-launch`**: `ensure_project_available_for_start` builds the askpass environment, resolves
  the OS user's agent socket, passes the operator identity the prompt is issued for, and bounds the wait.
- **`tddy-host-service`**: a prompt-raising entry point that is not `AddHostKey` (issue → await → decrypt →
  hand plaintext to the caller, without unlock-and-add); an authenticated loopback endpoint for the helper.
- **`tddy-service` (proto)**: a small askpass service message pair; no change to existing messages.
- **`tddy-agent-launch`** (and `tddy-session-agents`): the plain forwarded `StartSession` reuses
  `split_forward_deadline` instead of the 30 s default.
- **`tddy-web`**: prompt subscription and dialog mounted on the create-session path.
- **New package** for the askpass helper binary; `install`, `release`, `publish.sh`,
  `scripts/dev-runtime-binaries.sh` and the drift test that pins them.
- **Requires OpenSSH ≥ 8.4** on the host for `SSH_ASKPASS_REQUIRE=force`; older hosts report that the
  clone cannot ask for a passphrase rather than hanging.

### User Impact

- An operator with a passphrase-protected key sees a passphrase dialog naming the key, answers it once,
  and the session starts; later clones by that OS user need no prompt while the agent holds the key.
- An operator with an unreachable agent, an untrusted host or an unusable key gets a specific error in
  seconds instead of a five-minute `deadline_exceeded`.
- A forwarded start can stay "starting" for up to the clone budget while a prompt is outstanding.
- No migration. Hosts need OpenSSH ≥ 8.4 and a running agent for the OS user to benefit from prompting.

## Implementation Plan

1. Fail-fast clone environment (`ConnectTimeout`, `GIT_TERMINAL_PROMPT=0`) and the worker env channel.
2. Askpass helper binary and the daemon's loopback endpoint behind it.
3. Daemon raises the passphrase prompt for a helper request and returns the decrypted answer.
4. Clone wiring: `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE`, `AddKeysToAgent`, `SSH_AUTH_SOCK`, backends.
5. Longer forward deadline for clone-bearing starts.
6. Web prompt mount on the create-session path.
7. Packaging (`install`, `release`, `publish.sh`, dev-runtime list) and operator docs.

## Acceptance Criteria

- [ ] **AC1** — Cloning with a passphrase-protected key raises exactly one `SSH_KEY_PASSPHRASE` prompt for
      the operator who started the session; its subject is the key as ssh names it in its prompt (ssh cuts that to 100 characters, so it is
      display text, never a path the daemon opens).
- [ ] **AC2** — After the prompt is answered correctly the clone completes in the same attempt, and the key
      is present in the OS user's agent (`ssh-add -l` lists it).
- [ ] **AC3** — A second clone by the same OS user, with the key already in the agent, raises no prompt.
- [ ] **AC4** — A wrong passphrase yields a clone failure that says the passphrase was rejected (the helper
      reports "the previous passphrase for key … was rejected (incorrect passphrase)" when ssh re-asks for a
      key the daemon already answered); it does not hang and does not leak the answer into any log or error.
- [ ] **AC5** — A prompt not answered within its lifetime fails the clone with "nobody answered the
      passphrase prompt before it expired", well inside `spawn_worker_request_timeout`.
- [ ] **AC6** — ssh's non-passphrase askpass requests (host-key confirmation, password) are refused and the
      clone fails with ssh's own message within seconds.
- [ ] **AC7** — An unreachable host fails the clone within `ConnectTimeout` plus a small margin, naming the
      connection failure; an unusable key with no passphrase fails with `Permission denied (publickey)`.
- [ ] **AC8** — With no reachable ssh-agent for the OS user the SSH clone is still attempted, without
      prompting: a key that needs no passphrase works as it always did, and a failure says that no ssh-agent
      was reachable (or that the lookup could not be made, which is a different thing). The daemon starts no
      agent and never refuses the clone on that account alone.
- [ ] **AC9** — The prompt is delivered only to the GitHub user whose session started the clone, never to
      another operator on the host.
- [ ] **AC10** — The decrypted passphrase appears in no daemon log line at any level, no RPC response and no
      error string; the helper's endpoint accepts a request only with the clone's own token and refuses it
      after the clone ends. (Per clone, not per question: ssh re-asks up to three times after a wrong
      passphrase, all with the same token.)
- [ ] **AC11** — A `StartSession` — unary or streamed — forwarded to a peer daemon is bounded by the same budget a split session's
      forwarded start has — longer than the 120 s prompt lifetime — and not by the 30 s forward deadline. A
      streamed start stays alive for the same time because the peer sends a keep-alive event every 5 s while
      it provisions, so the relay's 30 s idle timeout is neither widened nor tripped.
- [ ] **AC12** — The create-session UI shows the passphrase dialog while a start is waiting on the prompt,
      encrypts the answer under the host's pinned public key, and clears the dialog when answered,
      expired or cancelled.
- [ ] **AC13** — The forked-worker backend and the supervised backend both carry the clone environment;
      under the supervisor a missing `allowed_env_keys` entry is reported as exactly that.
- [ ] **AC14** — The helper binary is installed beside `tddy-daemon` by `./install` and published by
      `./publish.sh`, and a dev run built with `--build` has it; the drift test pins all lists.
- [ ] **AC15** — A host whose OpenSSH is older than 8.4 reports that passphrase prompting is unavailable
      instead of hanging.

## References

- Initial discovery: [`docs/dev/1-WIP/2026-10-17-clone-passphrase-prompt-initial-discovery.md`](../../../dev/1-WIP/2026-10-17-clone-passphrase-prompt-initial-discovery.md)
- Host prompt channel: `packages/tddy-service/proto/host.proto` (`StreamHostPrompts`, `AnswerHostPrompt`)
- Agent socket resolution: `packages/tddy-host-service/src/ssh_agent.rs` (`WellKnownAgentSockets`)
- Related deferred work: [loading a key needs a privileged socket path](../../../dev/todo/2026-09-06-loading-a-key-needs-a-privileged-socket-path.md)
  (side-stepped for this path by ssh adding the key itself),
  [an unprivileged daemon cannot probe another OS user](../../../dev/todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md)

# Cloning over SSH: asking for a key passphrase

**Status:** Current

## Summary

A clone the daemon runs over SSH, as the OS user a session belongs to, asks the operator for the
passphrase of a protected key instead of waiting for input that cannot arrive. The question reaches the
operator who started the session as the same passphrase dialog the Hosts screen uses
([loading a key into a host's ssh-agent](../web/hosts-screen-add-key.md)); the answer is encrypted in the
browser under the host's public key and decrypted only by the daemon. ssh loads the unlocked key into
that OS user's own ssh-agent, so later clones and fetches by the same user ask nothing while the agent
holds the key.

Every other reason an SSH clone cannot proceed — an untrusted host, a key the server refuses, an
unreachable network — ends the clone within seconds with the cause in its error.

## What an operator sees

1. They start a session in a project the target host does not have yet. The form shows its usual
   starting state.
2. When the host's ssh asks for a key passphrase, the passphrase dialog opens over the form, naming the
   host and the key, with the host's key fingerprint for verification.
3. They type the passphrase. The clone completes, and the session starts.

On a split session (the codebase placed on another daemon) the dialog is the codebase host's, because
that host does the cloning.

A wrong passphrase is reported by the clone's error (`the previous passphrase for key '…' was rejected
(incorrect passphrase)`). ssh asks up to three times for one key, so the dialog opens again for each
attempt.

## How a clone gets its passphrase

1. The clone runs `ssh` with `SSH_ASKPASS` set to the `tddy-askpass` helper and
   `SSH_ASKPASS_REQUIRE=force`, so ssh calls it with no terminal and no `DISPLAY`.
2. For a key passphrase question the helper calls a loopback endpoint on the daemon with a token that
   belongs to this one clone.
3. The daemon raises the host prompt for the GitHub user whose session started the clone, waits for the
   encrypted answer, decrypts it in the daemon and returns it to the helper, which prints it for ssh.
4. ssh unlocks the key and, with `AddKeysToAgent=yes`, adds it to the OS user's ssh-agent. The daemon
   never opens that agent's socket.

The helper relays **only** a key passphrase question. Host-key confirmation and account password
questions are refused, so ssh fails with its own message.

## When the clone cannot ask

| Situation | What happens |
|---|---|
| The OS user has no reachable ssh-agent | The clone runs without prompting. A key with no passphrase works; a protected key fails fast and the error names the missing agent. A lookup that could not be made is reported as that, not as a missing agent. |
| The host's OpenSSH is older than 8.4, or its version cannot be read | The same: the clone runs without prompting, and a failure names the version and the 8.4 floor (`SSH_ASKPASS_REQUIRE=force` arrived in 8.4). |
| The operator never answers | The prompt expires after its 120-second lifetime and the clone fails, the helper saying the prompt expired unanswered. |
| A facilitator clone (the project is fetched from another daemon) | No SSH key is involved on the owning daemon, so none of this applies. |
| An HTTPS remote | No key is asked for. |

No ssh-agent is ever started by the daemon.

## Host requirements

- OpenSSH 8.4 or later on the host, for a protected key to be asked for.
- A running ssh-agent for the OS user, reachable at the places the Hosts screen already looks for it
  (the daemon's own `SSH_AUTH_SOCK` for its own user, `/run/user/<uid>/ssh-agent.socket`,
  `/run/user/<uid>/gnupg/S.gpg-agent.ssh`, `/run/user/<uid>/keyring/ssh`).
- The `tddy-askpass` binary beside `tddy-daemon` (shipped by `./install`, `./release` and
  `./publish.sh`).
- Under `tddy-supervisor`, the seven variables a clone sets listed in `spawn_policy.allowed_env_keys`
  (`GIT_SSH_COMMAND`, `GIT_TERMINAL_PROMPT`, `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE`, `SSH_AUTH_SOCK`,
  `TDDY_ASKPASS_TOKEN`, `TDDY_ASKPASS_URL`). The supervisor refuses a request naming an unlisted key,
  so without them every SSH clone through it is refused. The shipped configuration templates document the
  list.
- The host's `github.com` (or other forge) host key in the OS user's `known_hosts`. A host-key question is
  refused, not relayed.

## What the daemon guarantees

- **The passphrase is never at rest and never logged.** It exists decrypted in the daemon only between the
  decrypt and the response to the helper, is zeroed afterwards, and prints as `Passphrase(<redacted>)`.
- **One hop in the clear, locally.** The helper receives the passphrase over loopback HTTP, as the same OS
  user, authenticated by a token that lives as long as the clone and names its operator. A request with a
  token the daemon did not issue, or one whose clone has ended, is refused without raising a prompt.
- **A prompt belongs to one operator.** It is shown only to the GitHub user whose session started the
  clone, and only that user can answer it.
- **A forwarded start is not abandoned while someone types.** A `StartSession` forwarded to a peer has no
  overall deadline: it rides a streamed forward, and the peer sends a keep-alive event every 5 seconds for
  the whole life of the start — provisioning, a passphrase prompt, the worktree cut — so the relay's
  30-second per-frame idle timeout only ends a start whose peer has gone silent.

## Acceptance criteria

- [x] A protected key's passphrase is asked for exactly once per clone, as a key-passphrase prompt naming
      the key as ssh names it (ssh cuts the path in its question to 100 characters, so it is display text).
- [x] After a correct answer the clone completes in the same attempt and the key is in the OS user's agent.
- [x] A second clone by the same OS user, with the key in the agent, asks nothing.
- [x] A wrong passphrase fails the clone saying so, without hanging and without the answer in any log,
      response or error.
- [x] An unanswered prompt ends the helper's request as expired.
- [x] Host-key and password questions are refused and the clone fails within seconds.
- [x] An unreachable host fails within `ConnectTimeout` (15 seconds by default) naming the cause; a key the
      server refuses fails with `Permission denied (publickey)`.
- [x] With no reachable agent, or an OpenSSH older than 8.4, the clone runs without prompting and a failure
      says why.
- [x] The prompt reaches only the GitHub user whose session started the clone.
- [x] The decrypted passphrase appears in no log line, response or error; a token the daemon did not issue
      is refused; a token stops working when its clone ends.
- [x] A forwarded start, unary or streamed, has no overall deadline: the peer's keep-alives keep it open,
      and only 30 seconds of peer silence ends it.
- [x] The create-session form shows the dialog while a start waits, encrypts under the host's pinned key,
      follows the codebase host on a split start, and clears when answered, cancelled or ended.
- [x] The forked-worker and supervised backends carry the clone's environment, and the keys it may set are
      the documented allowlist.
- [x] `tddy-askpass` is installed beside the daemon and built wherever its sibling helpers are.

## Known limits

- The relayed forward and the keep-alive are pinned over a scripted in-memory peer on a paused clock; no test
  drives a forward over a real LiveKit room.
- Only the clone a session start runs asks. Other daemon-run git over SSH (fetch, push, worktree sync, the
  project coordinate handlers' clone) cannot ask; the first clone usually leaves the key in the agent, so
  they succeed afterwards.
- A supervisor refusal for an unlisted key is reported as the supervisor's own refusal, not as a missing
  `allowed_env_keys` entry.

## Out of scope

Starting or owning an ssh-agent. Answering host-key confirmations. Prompting for HTTPS credentials.
Storing a passphrase. Any ssh authentication method other than a key passphrase.

## Related

- [Loading a key into a host's ssh-agent](../web/hosts-screen-add-key.md) — the prompt channel, the
  encryption and the key-continuity check this feature reuses
- [Project concept](project-concept.md) — where a clone happens
- [Remote git repo](remote-git-repo.md) — the facilitator clone, which uses no SSH key on the owning daemon
- [Supervisor](../supervisor/tddy-supervisor.md) — `spawn_policy.allowed_env_keys`
- Dev: [`packages/tddy-askpass/README.md`](../../../packages/tddy-askpass/README.md),
  [`packages/tddy-host-service/docs/clone-askpass.md`](../../../packages/tddy-host-service/docs/clone-askpass.md),
  [`packages/tddy-spawn/docs/clone-ssh-environment.md`](../../../packages/tddy-spawn/docs/clone-ssh-environment.md)

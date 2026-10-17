# tddy-askpass

ssh's `SSH_ASKPASS` helper for a clone the daemon runs as an OS user.

When that user's key is passphrase-protected, ssh runs this program with its question as the only
argument. The helper relays a **key passphrase** question to the daemon's loopback askpass endpoint;
the daemon asks the operator who started the clone and returns the answer, which the helper prints
on stdout (one line) for ssh. ssh then loads the key into the user's ssh-agent itself.

## Environment contract

| Variable | Meaning |
|----------|---------|
| `TDDY_ASKPASS_URL` | Base URL of the daemon's loopback endpoint (`http://127.0.0.1:<port>`) |
| `TDDY_ASKPASS_TOKEN` | Token the daemon issued for this clone; names the operator the prompt is raised for |

Either missing: exit 1, stderr names the variable.

## Behaviour

- Any question that is not `Enter passphrase for key '...': ` (host-key confirmation, account
  passwords) is never relayed: exit 1, stdout empty.
- Answered: answer plus `\n` on stdout, exit 0, stderr empty.
- Refused, expired, unreachable or malformed reply: exit 1, stdout empty, stderr states the cause.
- The passphrase is written to stdout only, never to stderr.

## Who runs it

Not a human: the daemon sets `SSH_ASKPASS` to this binary (resolved beside the daemon executable,
then on `PATH`) in the environment of the clone it runs. It ships with `./install`, `./release`
and `./publish.sh` next to `tddy-remote-git-repo`.

When ssh asks again for a key this clone already got an answer for (after a wrong passphrase), the helper
also writes `the previous passphrase for key '…' was rejected (incorrect passphrase)` to stderr before it
prints the new answer; ssh passes that into the clone's error.

Behaviour: [cloning over SSH](../../docs/ft/daemon/clone-ssh-passphrase.md). The daemon's end:
[`tddy-host-service/docs/clone-askpass.md`](../tddy-host-service/docs/clone-askpass.md).

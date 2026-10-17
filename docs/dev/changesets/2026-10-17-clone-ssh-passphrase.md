# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

A daemon-run `git clone` over SSH as an OS user with a passphrase-protected key asks the operator for the
passphrase instead of hanging. ssh runs the new `tddy-askpass` helper, which calls a loopback endpoint on
the daemon; the daemon raises the existing host prompt for the GitHub user who started the session and
returns the decrypted answer; ssh adds the unlocked key to the OS user's own ssh-agent. An OpenSSH older
than 8.4, or no reachable agent, runs the clone without prompting. A forwarded `StartSession` waits the
split-start budget, and a streamed start sends a keep-alive while it provisions.

Packages: `tddy-askpass` (new), `tddy-host-service`, `tddy-spawn`, `tddy-agent-launch`,
`tddy-daemon-kernel`, `tddy-service`, `tddy-session-files`, `tddy-session-lifecycle`, `tddy-daemon`,
`tddy-web`, and the install, release, publish and test scripts.
Behaviour: [cloning over SSH](../../ft/daemon/clone-ssh-passphrase.md),
[the same question on the create-session form](../../ft/web/hosts-screen-add-key.md#the-same-question-while-a-session-starts).

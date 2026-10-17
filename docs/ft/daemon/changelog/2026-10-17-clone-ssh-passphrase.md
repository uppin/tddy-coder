# 2026-10-17 — A remote clone asks for an SSH key passphrase

- **A protected key no longer hangs the clone.** A clone the daemon runs over SSH asks the operator who
  started the session for the key's passphrase, in the same dialog the Hosts screen uses, instead of
  waiting five minutes for input that cannot arrive. ssh adds the unlocked key to the OS user's own
  ssh-agent, so later clones by that user ask nothing.
- **Everything else fails fast.** An unreachable host, a refused key and a host-key question end the
  clone within seconds with the cause in the error. With no reachable agent, or an OpenSSH older than 8.4,
  the clone runs without prompting and says why when it fails; a key with no passphrase keeps working.
- **A forwarded start waits for a person.** A `StartSession` forwarded to a peer waits the split-start
  budget (330 s by default) instead of 30 s, and a streamed start sends a keep-alive every 5 s while the
  host provisions.
- **Supervised hosts need seven environment keys allowed** (`GIT_SSH_COMMAND`, `GIT_TERMINAL_PROMPT`,
  `SSH_ASKPASS`, `SSH_ASKPASS_REQUIRE`, `SSH_AUTH_SOCK`, `TDDY_ASKPASS_TOKEN`, `TDDY_ASKPASS_URL`); the
  configuration templates list them.
- New binary `tddy-askpass`, installed beside the daemon.

See [cloning over SSH](../clone-ssh-passphrase.md).

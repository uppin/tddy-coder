# 2026-09-14 — List SSH config Host aliases (#ssh-exec 1/4)

**Type:** Feature

Stack node 1/4: `host.proto` gains `ListSshConfigHosts`; `tddy-host-service` parses `~/.ssh/config`
and serves the RPC; `tddy-web` shows aliases on the Hosts row. Session SSH execution and split
filtering remain later nodes. Recorded ⚠ DURING: supervised daemon reads config as its service user
([2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md](../todo/2026-09-06-unprivileged-daemon-cannot-probe-another-os-user.md)).

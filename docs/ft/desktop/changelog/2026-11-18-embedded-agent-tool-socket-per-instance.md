# 2026-11-18 — A second instance no longer takes the first one's sessions down

- Each instance serves its sessions an agent-tool socket of its own. Starting and stopping a
  second Tddy Desktop on the same `~/.tddy` no longer leaves the first one's sessions refused with
  `Connection refused`.
- `tddy-desktop --help` and `--version` answer and exit, and other arguments are refused. None of
  them opens the application.
- On exit the application stops its workspace jails and index daemon and removes its socket.

# tddy-users-file

The `users.yaml` model, the GitHub login → OS user mapping, and the one validation both of its
readers apply: `tddy-supervisor`, which uses only the `os_user` column, and the daemon
(`tddy-daemon-kernel`), which uses the logins.

- `UsersMapping::from_yaml` covers the shape, no `root`, no empty login, one OS user per login, at
  most `MAX_MAPPED_OS_USERS` (15) distinct OS users, and each OS user being one safe path and
  descriptor-name segment.
- It reads no file and checks no ownership. Each reader keeps its own trust check, because they trust
  different owners.
- It depends on `serde` and `serde_yaml` only, and names no coding agent or daemon type.

Introduced by the `#agent-tools` sweep (#876). Its API is a contract stub until that PR's green phase.

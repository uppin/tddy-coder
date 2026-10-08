# Commit identity over the spawn wire

A tool session (`tddy-coder`) is started by `ToolSpawnPlan` → `SpawnOptions` → one of three backends: the
in-process `spawn_as_user`, the forked `spawn_worker` over a JSON pipe, or `tddy-supervisor` over its own
wire. A session whose project acts as an account carries **the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*`
pairs** of that account, on start and resume. The pairs come from one
`SessionAccountAccess::session_identity` lookup in `tddy-session-lifecycle`
([session identity](../../tddy-session-lifecycle/docs/session-identity.md)); a refused resolution
sends none and the session starts under the checkout's own identity.

## Carriers

| Surface | Field |
|---|---|
| `SpawnRequest` (the worker's JSON request) | `git_environment: Vec<(String, String)>`, `#[serde(default)]` |
| `SpawnOptions` | `git_environment: &[(String, String)]` (`Default` = empty) |
| `SessionChildPlan` | `env`, applied by `spawn_as_user` (`Command::envs`) |
| supervised path | added to the `SpawnSession` request's `env` (`supervisor_spawn::with_commit_identity`); the supervisor's `SpawnSessionRequest` already carries an `env` map |

## Compatibility

The daemon, its forked worker and the supervisor may be at different versions. A new daemon talking to an
old worker is ignored by serde, and the child starts without the pairs (the same outcome as a refused
resolution). An old daemon talking to a new worker omits the field, which deserializes as empty.

## No credential on this wire

The field's contract is the four pairs. `plan_session_child` refuses any other key
(`SESSION_GIT_ENVIRONMENT_KEYS`) before any I/O, naming the key and never the value, because the
supervisor wire's `env` is a generic map that a future caller could otherwise put a credential on. The
account's token reaches a session's tools only by being asked for, per call.

`spawn_as_user` also removes `GITHUB_TOKEN` and `GH_TOKEN` from the child's environment: it does not
`env_clear`, so a daemon with either exported would otherwise hand it to every child by ordinary
inheritance. The forked worker goes through the same function. The supervisor path starts the child from
the supervisor's own environment plus the request's `env`; the supervisor's environment is the
operator's configuration and is not touched here.

## Operator consequence

The supervisor's `spawn_policy.allowed_env_keys` refuses a request naming an unlisted key, so on a
supervised host the four keys must be listed ([host sockets](../../tddy-supervisor/docs/host-sockets.md#configuration)).

## Tests

`tests/session_git_environment.rs` (a planned child is given exactly the four pairs, or none; a GitHub
token and a loader variable are refused as session environment; a spawned child starts with the pairs in
its environment and is not handed the daemon's `GITHUB_TOKEN` / `GH_TOKEN` by inheritance) and
`spawn_worker.rs` (the request carries the pairs, round-trips over the worker pipe, and an old payload
decodes empty). That the account's token is in no variable a tool session starts with is pinned in
`tddy-session-lifecycle/tests/tool_session_git_identity.rs`.

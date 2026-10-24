# The SSH environment of a clone

A clone the daemon runs as an OS user is given its environment by a pure function, `clone_env::CloneSshEnv`,
so what reaches `git` is a value a test can read, the same whichever backend carries it. Behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md).

## What it sets

| Variable | Value |
|---|---|
| `SSH_ASKPASS` | the `tddy-askpass` helper's path |
| `SSH_ASKPASS_REQUIRE` | `force`, so ssh calls the helper with no terminal and no `DISPLAY` (OpenSSH 8.4) |
| `GIT_TERMINAL_PROMPT` | `0`, so git never prompts for itself |
| `GIT_SSH_COMMAND` | `ssh -o AddKeysToAgent=yes -o ConnectTimeout=<n>` (15 s by default) then any extra arguments |
| `SSH_AUTH_SOCK` | the OS user's agent socket; **unset** when there is none, since an empty value is a different failure |
| `TDDY_ASKPASS_URL`, `TDDY_ASKPASS_TOKEN` | where the daemon's endpoint is, and which clone this is |

`without_prompting()` is the variant for a clone that must not ask: it adds `-o BatchMode=yes` to
`GIT_SSH_COMMAND` and omits the askpass variables, so a key with no passphrase works and anything else fails
fast.

`ssh_args` extra arguments are quoted for the shell git runs `GIT_SSH_COMMAND` through (`shell_word`): left
alone when made only of characters a shell never splits, single-quoted otherwise. A path with a space or a
quote stays one word. The method has no production caller; the real-process acceptance suite uses it to
aim ssh at a fixture server.

## The allowlist

`CloneSshEnv::ALLOWED_ENV_KEYS` is every key `into_env` can set, sorted, and a test holds it equal to what
a clone sets. Under `tddy-supervisor`, `spawn_policy.allowed_env_keys` must list them: the supervisor
refuses a request naming an unlisted key
([host sockets](../../tddy-supervisor/docs/host-sockets.md#configuration)). The configuration templates
(`supervisor.yaml.production`, `dev.supervisor.yaml`, `daemon.yaml.production`) document the list.

## Carriers

| Backend | How the environment travels |
|---|---|
| in-process | `clone_run::clone_with_progress` (and its thin wrapper `spawner::clone_as_user_with_env`) sets it on the `git` child |
| forked worker | `CloneRequest.env`, a `BTreeMap<String, String>` marked `#[serde(default)]`; the worker runs `CloneRequest::run`, which calls `clone_with_progress` |
| supervisor | `supervisor_spawn::clone_repo_via_supervisor_with_env` puts it in the request's `env` |

A request from a daemon that predates the field decodes with an empty environment.

## `OpenSshVersion`

`OpenSshVersion::parse(banner)` reads `ssh -V` (`OpenSSH_8.3p1 Ubuntu…`, `OpenSSH_10.3p1, LibreSSL…`) into
numbers, so 10.3 is newer than 8.4; a banner that is not OpenSSH is `None`.
`supports_forced_askpass()` is true from 8.4. The daemon asks once per process and clones without
prompting when the answer is no or unknown.

## Testing

`tests/clone_env.rs` (each variable, the unset socket, the timeout, extra arguments and their quoting, the
allowlist), `tests/clone_request_env.rs` (the field round-trips; an old payload decodes empty) and
`tests/clone_env_openssh_version.rs`.

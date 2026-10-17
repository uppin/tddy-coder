# 2026-10-17 — A clone asks for an SSH key passphrase

**Type:** Feature

`clone_env`: `CloneSshEnv` (the askpass, agent and fail-fast SSH settings of a clone, with `ALLOWED_ENV_KEYS` and `without_prompting`) and `OpenSshVersion`. `CloneRequest` carries `env` (`#[serde(default)]`), which the forked worker applies through `clone_as_user_with_env`. See [the SSH environment of a clone](../clone-ssh-environment.md).

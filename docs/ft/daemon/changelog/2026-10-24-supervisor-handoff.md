# 2026-10-24 — A supervised daemon knows it is supervised

`tddy-supervisor` now tells every managed service it starts where its privileged socket is
(`TDDY_SUPERVISOR_SOCKET`, set after the service's `env:` overlay so config cannot forge it). A daemon
started that way spawns sessions and clones repos through the supervisor; started any other way (Tddy
Desktop, `./install --user`, by hand) it forks its own spawn worker. The daemon removes the variable from
its own environment at startup, so nothing it spawns itself inherits it. `daemon.yaml`'s `supervisor:`
block is gone: a file that still has one fails to load, naming the block as obsolete and saying to delete
it, and `./install` (system and `--user`) removes it from an existing `daemon.yaml` on upgrade and says
so. A host can no longer be supervised in fact and unsupervised in config — the state that broke cloning
on a supervised host. An unreachable supervisor still fails the operation closed. See
[systemd install](../systemd-install.md) and [tddy-supervisor](../../supervisor/tddy-supervisor.md).

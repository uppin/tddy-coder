# Supervisor product area changelog

**Merge hygiene:** [Changelog merge hygiene](../../dev/guides/changelog-merge-hygiene.md) — newest **`##`** first; **distinct titles** when two releases share a date; single-line bullets; do not edit older sections for unrelated work.

## 2026-08-03 — tddy-supervisor confines root privilege to a small brokered mini-init

- systemd now starts one root unit, `tddy-supervisor.service`; `tddy-daemon` runs as `tddy` as its declared unprivileged child, and an inherited `tddy-daemon.service`/`.socket` is stopped, disabled and masked — socket first, because `disable` does not disarm a listening socket.
- The privileged surface is a root-owned unix socket gated by `SO_PEERCRED` against declared-service uids, then by a root-owned policy file: declared-service control, `SpawnSession`, `SpawnSandbox`, session status/stop, and cgroup v2 scope lifecycle.
- Nothing is granted by default: with no `spawn_policy` the supervisor supervises the daemon and refuses every spawn. Resource limits are clamped down to ceilings rather than rejected, and an omitted limit still receives the ceiling.
- Every refusal renders exactly `"request denied"`, so an error cannot reveal whether a user, path or scope exists; an allowlisted account missing from the host is refused identically to one never listed.
- `allowed_env_keys` denies an unlisted key rather than dropping it, and loader keys (`LD_*`, `MALLOC_*`, `GCONV_PATH`, `NLSPATH`, `LOCPATH`, `HOSTALIASES`, `RESOLV_HOST_CONF`) can never be listed — `LD_PRELOAD` on an allowlisted tool would make the tool allowlist meaningless.
- The supervisor binds the daemon's client socket as root before the daemon starts and hands it over as fd 3, replacing what `tddy-daemon.socket` did; the daemon needed no changes, and the listener is held across restarts so a client never meets an unlinked path.
- `SpawnSandbox` builds a real jail — user/mount/net namespaces, private root, bind mounts, loopback — with privilege surrendered before any namespace exists, so the uid map is the rootless one rather than mapping the child to host root.
- Bind-mount sources are resolved with `openat2(RESOLVE_NO_SYMLINKS | RESOLVE_BENEATH)` in the child immediately before the bind, so a session user cannot escape an allowed mount root through a symlink; a kernel that cannot do it refuses the mount rather than falling back.
- The supervisor prepares the cgroup v2 subtree it owns at startup and refuses to run if it cannot — a scope whose `memory.max` has no controller behind it would report a ceiling as applied that the kernel does not enforce.
- Session and repo-clone spawning route through the supervisor when one is configured, and fail closed when it is unreachable; sandbox, claude-cli, cursor-cli and PTY sessions still spawn from the daemon and are tracked in `docs/dev/TODO.md`.
- `./install --systemd --user` installs a per-user daemon unit and no supervisor: rootless it could neither `setuid` nor delegate cgroups nor own a root socket, so it would broker nothing.

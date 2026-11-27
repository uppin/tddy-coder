# 2026-11-28 — Supervisor and spawn go through the Fs port

The production disk I/O of `tddy-supervisor` and `tddy-spawn` goes through the `tddy-fs`
port (refactor, no behaviour change): config loading, the cgroup broker, socket binding,
spawner entry points and clone runs run behind held handles and `<fn>_on` seams with
`DiskFs` only at composition roots. Descriptor-bound operations (socket-directory
descriptors, child stdio, pipe ends) stay on `libc`/`OwnedFd`. No port surface was added.

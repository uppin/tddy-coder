# 2026-10-04 — How long a daemon waits for a peer is an operator setting

- **`peer_forward_timeout_secs`** bounds how long a daemon waits for a peer in the common room to
  answer a forwarded unary RPC, or to open a forwarded server stream. Default **30** — a daemon with no
  new key behaves as before; a value of `0` is read as one second. A timeout is `DEADLINE_EXCEEDED`
  naming the seconds waited. See
  [livekit-peer-discovery.md § Configuration](../livekit-peer-discovery.md#configuration).
- **The remote managed worktree's split start follows it:** its forward waits
  `spawn_worker_request_timeout + peer_forward_timeout_secs`, so the roughly 330 s surfacing time of a
  vanished codebase daemon is for the default setting. See
  [remote-managed-worktree.md](../remote-managed-worktree.md).
- **Not part of the setting:** the idle wait between frames of a forwarded stream stays fixed at 30 s,
  and a peer that has left the room is still refused immediately.
- `daemon.yaml.production` carries the key as a commented example.

# 2026-11-20 — A running tool session lists active, and delete and signal reach its tddy-coder through the daemon

A tool session's `tddy-coder` is held by the daemon as a process tab until the backend that spawned it
(the supervisor, the spawn worker or a direct spawn) reports that it exited. A running tool session lists
active, an exited one inactive, and delete, `SignalSession` and `TerminateSession` act on the held child
instead of the pid its `.session.yaml` records. The supervisor gains a `SignalSession` RPC for the child it
owns. See [session-liveness.md](../session-liveness.md).

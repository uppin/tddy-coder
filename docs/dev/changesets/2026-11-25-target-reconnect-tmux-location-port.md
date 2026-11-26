# 2026-11-25 — The TmuxLocation port: one detach and re-attach lifecycle for every tmux run (node 1/3)

**Type:** Architecture

Node 1 of the `#target-reconnect` stack. `tddy-cli-sessions` gains the `TmuxLocation` port (`tmux_location.rs`): a location answers whether a named tmux session lives (`Err(TmuxUnreachable)` — could not tell — is never read as gone), builds the client attached to exactly that session, and kills it. `HostTmux` wraps the host helpers as the daemon's OS user; `FakeTmuxLocation` serves watcher and classifier tests. `TmuxRun` carries the location, so `DetachWatch`, `classify_ended_main` and the attach/re-attach paths probe and attach through the port, and the location travels inside `TmuxRun` rather than as a new spawn parameter.

While a location cannot be asked, the detach watcher doubles its probe interval from 10 s to a 5 min cap and holds the session live for at most 15 min of consecutive unreachability; past the grace it releases only the hold — no agent end is announced and no state is dropped — and keeps probing. A refused host probe no longer reads as "gone" and no longer drops the session's state (D6).

Nodes 2 (`feature/target-reconnect/target-exit-lazy-reattach`: `TargetTmux`, `owner_subject`, target client exit and lazy re-attach) and 3 (`feature/target-reconnect/startup-reattach-target-chains`: startup re-attach of target chains) build on this port. See [session-liveness.md](../../ft/daemon/session-liveness.md) and `packages/tddy-cli-sessions/docs/module-layout.md`.

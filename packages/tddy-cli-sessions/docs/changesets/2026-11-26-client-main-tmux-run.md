# 2026-11-26 — A client main and tab carry their TmuxRun (#target-reconnect 2/3)

**Type:** Architecture

`ClientMain` replaces `spawn_rendered_client_main`'s positional arguments and carries the main's `TmuxRun`; `ClientTab` gains the same `TmuxRun`, so a transport client that is a client of a tmux session — on a target, or here — is classified and watched through that run's location when it ends. `TmuxRun` is public with `TmuxRun::with_location`. `ReattachSkip` gains `NoOwnerSubject` and `TargetUnreachable(String)` (and is no longer `Copy`). Part of #1041.

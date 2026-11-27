# 2026-11-28 — Activity, usage and logs go through the Fs port

The production disk I/O of `tddy-session-activity`, `tddy-usage`, `tddy-log` and
`tddy-cli-sessions` goes through the `tddy-fs` port (refactor, no behaviour change): session
stores, the notification log, usage accounting, log files and the tmux pane-env file run behind
injected handles and `<fn>_on` seams with `DiskFs` only at composition roots — service/store
constructors and the daemon's wiring. The port itself grows one additive method:
`Fs::create_new_with_mode` (exclusive create at exactly a mode), with `DiskFs` + `MemFs`
implementations and conformance tests.

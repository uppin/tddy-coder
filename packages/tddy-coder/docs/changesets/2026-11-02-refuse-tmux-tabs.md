# 2026-11-02 — A tool session refuses tmux

**Type:** Feature

The participant's terminal coordinate passes `NoTmux`, so `ListTmuxSessions` is `FAILED_PRECONDITION`,
and `CoderTerminalRoster` refuses kind `tmux` the same way before anything spawns. Both pinned in
`connection_service_participant` tests; `two_server_parity_acceptance` builds both servers with
`NoTmux`. Part of [#806](https://github.com/uppin/tddy-coder/pull/806).

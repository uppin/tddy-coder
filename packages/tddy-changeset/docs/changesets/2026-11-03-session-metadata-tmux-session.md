# 2026-11-03 — `SessionMetadata.tmux_session`

**Type:** Feature

`tmux_session: Option<String>` (`skip_serializing_if = "Option::is_none"`): the tmux session a tmux-run agent runs in; for such a session `pid` is the pane's. Part of [#814](https://github.com/uppin/tddy-coder/pull/814).

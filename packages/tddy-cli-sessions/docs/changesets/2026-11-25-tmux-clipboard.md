# 2026-11-25 — An agent tmux session forwards programs' clipboard writes

**Type:** Fix

`start_agent_tmux_session` enables clipboard forwarding on the session it starts
(`enable_tmux_clipboard`): `allow-passthrough` on the session's own window
(`allow_passthrough_args`, `=<name>:`), so tmux-aware programs' DCS-wrapped OSC 52 writes
pass through, and the server's `set-clipboard` upgraded from the default `external` to `on`
(`show_set_clipboard_args` / `parse_set_clipboard` / `set_clipboard_needs_enabling` /
`set_clipboard_on_args`), so plain writes are forwarded. Only `external` is upgraded: `on`
needs nothing, an explicit `off` is the user's hardening and is left alone, and an unreadable
value is not touched. tmux offers no session-scoped knob for plain writes (on 3.6a a session
target is accepted but sets it globally anyway), so the upgrade is server-wide at runtime; the
user's `~/.tmux.conf` is never touched. A session that refuses still runs its agent: the
failure is logged (`tddy_daemon::tmux`), not fatal, so an old tmux forwards what it can.

Tests: `tmux.rs` unit tests (arg shapes, the show parse, the upgrade rule) and
`tests/tmux_clipboard.rs` (real tmux on a private server: a plain write, a wrapped write and a
copy-mode copy reach the attached client; a start enables both options; an explicit `off`
stays off).

Cross-package record: [docs/dev/changesets/2026-11-25-tmux-clipboard-forwarding.md](../../../../docs/dev/changesets/2026-11-25-tmux-clipboard-forwarding.md).

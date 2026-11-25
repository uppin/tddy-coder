# 2026-11-25 — Clipboard forwarding for tmux on a target

**Type:** Fix

The target tmux commands announce the clipboard feature (`-T clipboard`), as host tabs do:
`target_tmux_tab_command` and `remote_agent_tmux_command`. An agent tab
(`launch_agent_tab_on_target` with `tmux_session`) runs `target_agent_tmux_tab_command`:
create detached, enable forwarding on the session, then attach as a clipboard client — a
session already living is attached to with forwarding enabled on it all the same, its
duplicate-session complaint silenced. The enablement is `target_tmux_clipboard_fragment`
(window `allow-passthrough`, server `set-clipboard` from `external` to `on`, an explicit
`off` left alone — the host's rule, in the target's shell because no tddy binary runs
there). The agent main keeps its `new-session -A` shape and runs the same fragment through
`enable_target_tmux_clipboard` once the agent is known to have started
(`enable_target_tmux_clipboard_logged`: a refusal is logged, not fatal). A person's own
sessions (`TargetTmux`/`TargetTmuxNew`) keep the target login's tmux configuration untouched.

Tests: the `-T` pins, the agent-tab command pinned and executed against a recording
stand-in tmux (default enables, `off` respected, duplicate still attaches), the main's
enablement through `InMemoryShell`, and `remote_agent_launch_acceptance` asserting both
options on the fake target's real tmux after a launch.

Cross-package record: [docs/dev/changesets/2026-11-25-tmux-clipboard-forwarding.md](../../../../docs/dev/changesets/2026-11-25-tmux-clipboard-forwarding.md).

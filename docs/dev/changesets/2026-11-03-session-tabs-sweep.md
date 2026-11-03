# 2026-11-03 — `#session-tabs` sweep: logic fixes and backlog records

**Type:** Bug Fix

Follow-up to the merged `#session-tabs` stack (#804–#810). Web: `initialContent` written once, so an OSC 52 in it copies once; the tmux "+" section says the host is not connected instead of loading for ever. Terminal RPC: the PTY spawn shares the kernel's privilege plan and `CannotBecomeUser` refusal with the tmux listing.

Decision (developer, 2026-11-03): option (c) — ship the shared decision and keep `setpriv`; a `pre_exec` drop in the PTY child would need portable-pty patched (a) or an owned `openpty` (b), and a Linux-as-root run.

Backlog: resolved and deleted — *GhosttyTerminal initial content written twice* (`2026-11-02-ghostty-terminal-initial-content-written-twice`). Narrowed, defer consent recorded — `2026-11-02-tmux-listing-and-tab-use-two-privilege-drops` (mechanism only). Deferred with consent — `2026-11-02-osc52-clipboard-reachability-unverified`. Added — `2026-11-03` "+" menu stays open when "+" scrolls away. Unaffected — `2026-09-06-unprivileged-daemon-cannot-probe-another-os-user`. Code issues in `tddy-agent-launch`, `tddy-daemon-sandbox`, `tddy-task`, `tddy-terminal-rpc`, `tddy-web` re-measured and deferred by the developer. Part of [#813](https://github.com/uppin/tddy-coder/pull/813).

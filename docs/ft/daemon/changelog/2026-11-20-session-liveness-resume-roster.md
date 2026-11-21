# 2026-11-20 — Resume restores every tab of a session, each under its recorded id

Resume brings back the whole [tab roster](../session-liveness.md#resume-restores-the-roster), not only the
main agent: agent tabs resume their own conversation where the harness can, a tmux-wrapped tab is
re-attached (or relaunched when its tmux session is gone), shells reopen fresh, and a tab that cannot be
restored is dropped and named. `ResumeSessionResponse` reports one `RestoredTab` per entry, the strip keeps
the active tab selected, and a tab reopened fresh or dropped shows a notice.

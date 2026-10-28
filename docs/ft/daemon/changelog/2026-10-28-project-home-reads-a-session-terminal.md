# 2026-10-28 — The project-home agent sees what a session's terminal shows

The project-home agent can read another session's screen with `ReadSessionTerminal(session_id)`,
on this host or another (`daemon_instance_id`): the visible rows of the main terminal's active
screen — the alternate screen while a full-screen TUI runs — each trimmed, with the cursor's row,
column and visibility, the grid size and the position in the output stream the text corresponds to.
It is the screen a reconnecting browser is repainted with, as text. Reading sends no input and
disturbs no attached screen. A session with no live terminal, or whose terminal has not reported its
size, is refused with the reason ([#767](https://github.com/uppin/tddy-coder/pull/767)). See
[Project concept § Reading a terminal](../project-concept.md#reading-a-terminal).

# 2026-10-28 — The project-home agent types into a session's terminal

The project-home agent can take control of another session's terminal with
`ClaimSessionTerminal(session_id)` and type into it with `SendSessionKeys(session_id, text?, keys?)`,
on this host or another (`daemon_instance_id`). Claiming takes the control lease the way a browser's
steal does, so a person watching sees who took control and takes it back by claiming it themselves;
every send after that is refused, naming the holder, until the agent claims again. Text is typed as a
bracketed paste when the terminal has enabled one; named keys (`Enter`, `Escape`, `C-c`, `F1`, …)
follow in order, and an unknown name sends nothing. Jailed terminals follow the same rule
([#770](https://github.com/uppin/tddy-coder/pull/770)). See
[Project concept § Typing into a terminal](../project-concept.md#typing-into-a-terminal).

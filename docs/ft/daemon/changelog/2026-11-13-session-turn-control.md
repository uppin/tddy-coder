# 2026-11-13 — Orchestrators give their sessions turns and resume them

An orchestrator (main, or any orchestrator session) drives a session the way an agent drives a subagent.

- **`SendSessionMessage`** gives a running session one new turn, the way its agent's YAML spec declares (`turn_input`; claude-cli pastes into the terminal and presses Enter). It takes no terminal control: a person watching in the browser keeps it.
- A stopped session is refused naming `ResumeSession`; one waiting on a permission prompt or a question is refused naming the answer tool; an agent that declares no `turn_input` is refused naming the agent.
- **`ResumeSession`** does what the web's Resume does, optionally followed by a first message. A running session answers `running` and is left alone.
- Project notification records name the session's orchestrator, and `WaitProjectNotifications` / `ReadProjectNotifications` take `started_by_me: true`, so an orchestrator waits on its own sessions' turns only.
- Both tools reach a session on another host.

See [Project concept § Sending a session a turn](../project-concept.md#sending-a-session-a-turn).

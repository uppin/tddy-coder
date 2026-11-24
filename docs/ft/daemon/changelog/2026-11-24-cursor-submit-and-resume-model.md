# 2026-11-24 — A Cursor session's turn is submitted, and a resume keeps the model picked in its TUI

- `SendSessionMessage`, `ResumeSession`'s first message, a watch wake and `SendSessionKeys` with text and keys press the submit key (or the keys) once the agent has echoed the text. Cursor Agent left a turn sent with Enter in one write sitting in its input box, locally and on a Coder target.
- No echo within 5 seconds: the keys are pressed anyway, and the answer says so — `echo_seen: false`. The three tools answer `echo_seen` whenever keys followed text.
- A model picked with `/model` inside a Cursor or Claude Code session is recorded as the session's model, so **Resume** relaunches on it rather than on the one it started with. Cursor's `auto` is recorded as it reports itself, `auto-smart`.
- One orchestrator's watch wake never waits on another's, and a wake in progress does not hold up the project's other watch calls.

Feature: [project-concept.md § Typing into a terminal](../project-concept.md#typing-into-a-terminal) · [§ Sending a session a turn](../project-concept.md#sending-a-session-a-turn) · [§ Watching sessions](../project-concept.md#watching-sessions) · [cursor-cli-session.md § Resume](../cursor-cli-session.md#resume-connect-signal-delete)

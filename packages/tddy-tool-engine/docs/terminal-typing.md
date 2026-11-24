# Typing into a terminal (`tddy_tool_engine::terminal_keys`)

## Role

`terminal_keys` turns a request to type — optional text, then named keys — into terminal input
bytes, and types them into a live terminal so that keys meant to act on the text (Enter, above all)
are read after it. Every path that types into an agent's terminal goes through it:

| Caller | Tool | Adapter |
|---|---|---|
| `tddy-tool-engine` (`project_home_terminal_tools.rs`) | `SendSessionKeys` | `LeasedTerminal` |
| `tddy-daemon-rpc` (`exec_tool/project_home/turn.rs`, `paste_and_submit_to`) | `SendSessionMessage`, `ResumeSession`'s first message, the watch waker's wake | `LiveTerminal` |
| `tddy-session-driver` (`server.rs`) | the driver's `send_keys` MCP tool | `DriverSession` |

The product rules are in
[project concept § Typing into a terminal](../../../docs/ft/daemon/project-concept.md#typing-into-a-terminal)
and [§ Sending a session a turn](../../../docs/ft/daemon/project-concept.md#sending-a-session-a-turn).

## Encoding

- **`encode_keys(text, keys, bracketed_paste) -> Result<Vec<u8>, String>`** — the bytes, in one
  buffer. `text` goes first: wrapped in `ESC[200~` … `ESC[201~` when the terminal enabled bracketed
  paste, otherwise raw, each line break (`\n` or `\r\n`) typed as `\r`. Each key name is then
  resolved through `tddy_coding_agent_specs::terminal_keys::named_key_bytes`, the one key-name table
  (an agent spec's `turn_input.submit_key` is validated at load against the same list). Refused,
  with nothing produced: no text and no keys; an unknown key name (named in the message); a text
  holding the paste-end sequence `ESC[201~` while pasting. An empty `text` counts as none.
- **`encode_typing(text, keys, bracketed_paste) -> Result<Typing, String>`** — the same refusals and,
  joined, the same bytes as `encode_keys`, kept as the two writes they are typed in:
  `Typing { text, keys }`, either possibly empty. `Typing::len` is both writes' byte count.

## Typing

**`type_into(terminal: &dyn TypingTerminal, typing: Typing, wait: &EchoWait) -> Result<Typed, TypingError>`**

- **Text and keys:** read the terminal's output offset, write the text, then poll the offset until
  it has moved past where it stood before the write (the application echoed the text) and then
  stayed still for `quiet` — or until `settle_cap` has passed since the echo began, for a screen
  that never holds still (a spinner). Only then write the keys.
- **No echo within `no_echo_timeout`: the keys are written anyway** — a deliberate, operator-approved
  choice, so an application that draws nothing as it is typed into still gets its submit key. It
  logs a `warn` and answers `echo_seen: Some(false)`.
- **Text alone, or keys alone:** one write, nothing waited on, `echo_seen: None`.

Why the wait: an application may read bytes arriving in the same read as the end of a paste as part
of it — Cursor Agent stores what trails `ESC[201~` and prepends it to its *next* input, so a paste and
Enter in one read leave the text unsubmitted. Writing them apart is not enough: a transport in
between (tmux, `coder ssh`) merges writes made back to back into one read. The echo is the only
evidence the application has read the text.

| Type | Holds |
|---|---|
| `TypingTerminal` (async trait) | the port: `output_offset() -> Result<u64, String>` (the terminal's output sequence point, `TextGrid::offset`, which moves whenever the application writes to its screen) and `write(bytes)` (one write to its input) |
| `EchoWait` | `no_echo_timeout` (default 5 s), `quiet` (30 ms), `settle_cap` (500 ms), `poll` (10 ms) |
| `Typed` | `sent_bytes` (both writes) and `echo_seen: Option<bool>`. `Typed::answer()` is the typing tools' answer: `{sent_bytes}`, plus `echo_seen` when it is `Some` — `SendSessionKeys` and the driver's `send_keys` both answer it |
| `TypingError` | how far typing got: `NothingSent` (the first write, or the offset read before it, failed) or `KeysNotSent` (the text was typed; reading the echo or writing the keys failed). Its `Display` says which |

## The adapters

- **`LeasedTerminal`** (`project_home_terminal_tools/leased_terminal.rs`, `pub(super)`) — a target's
  main terminal under a home session's lease: the offset from
  `ProjectHomeTerminalPort::read_terminal` (a whole `TextGrid`; the port has no offset-only read),
  each write through `ProjectHomeTerminalPort::send_keys`, which checks the lease on every write.
- **`LiveTerminal`** (`tddy-daemon-rpc`, `exec_tool/project_home/turn.rs`) — a session's
  `TerminalSession`: the offset is the capture's `end_offset()`, each write a `send_input(…, 0)`, with
  no lease read or changed. `paste_and_submit_to` reports `echo_seen` to its callers.
- **`DriverSession`** (`tddy-session-driver`) — the driven session: the offset from its grid, each
  write a `send`.

## The turn tools' answers

`project_home_turn_tools` carries the echo through its port types: `TurnDelivery::TerminalPaste
{ echo_seen }` (`TurnDelivery::echo_seen()` is `None` for `AcpPrompt`) and `ResumeOutcome::Resumed
{ message: Option<TurnDelivery> }` (`ResumeOutcome::echo_seen()` is `None` when no message was
pasted, and for `Running`). `SendSessionMessage` and `ResumeSession` add `echo_seen` to their answer
when it is `Some`.

## Tests

`tests/terminal_typing.rs` drives `type_into` against a fake TUI that reads its input in batches —
writes made back to back merge into one read — and, like Cursor, holds what trails `ESC[201~`: a paste
and Enter written at once stay in the input box; typed text is submitted once echoed; the keys after
the timeout with no echo; at the settle cap on a screen that never holds still; keys alone and text
alone as one write; `NothingSent` and `KeysNotSent`; `encode_typing` keeping the two apart, raw text's
line breaks staying in the text write. Uses `tokio`'s `test-util` paused clock (a dev-dependency
feature). `tests/project_home_turn_tools.rs` pins `echo_seen` in the turn tools' answers.

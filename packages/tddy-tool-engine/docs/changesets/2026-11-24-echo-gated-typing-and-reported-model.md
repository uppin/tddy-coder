# 2026-11-24 — Keys typed after text wait for the application's echo

**Type:** Fix

`terminal_keys` gains `encode_typing` (`Typing { text, keys }`, two writes), `type_into` over the
`TypingTerminal` port, `EchoWait` (5 s no-echo timeout, 30 ms quiet, 500 ms settle cap, 10 ms poll),
`Typed` with `Typed::answer()` and `TypingError` (`NothingSent` / `KeysNotSent`). The keys after a
text are written once the application has echoed it, or after 5 s with no echo, when they are
pressed anyway (operator decision) — a paste and Enter in one read left Cursor's input unsubmitted.
`SendSessionKeys` types through `LeasedTerminal` (`project_home_terminal_tools/leased_terminal.rs`)
and answers `echo_seen` when keys followed text. `TurnDelivery::TerminalPaste { echo_seen }` and
`ResumeOutcome::Resumed { message }` carry the echo to `SendSessionMessage` / `ResumeSession`
answers. `tokio`'s `test-util` added to dev-dependencies (`tests/terminal_typing.rs`).

Doc: [terminal-typing.md](../terminal-typing.md). Full record: [cross-package entry](../../../../docs/dev/changesets/2026-11-24-cursor-tui-submit-and-resume-model.md).

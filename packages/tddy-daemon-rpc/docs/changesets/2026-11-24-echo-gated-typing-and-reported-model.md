# 2026-11-24 — Turns pasted with the submit key after the echo; one task per watch wake

**Type:** Fix

- `paste_and_submit_to` (`exec_tool/project_home/turn.rs`) types through `encode_typing` +
  `type_into` over `LiveTerminal` (offset: the capture's `end_offset`; writes: `send_input(…, 0)`)
  and answers whether the echo was seen; `SendSessionMessage` and `ResumeSession`'s first message
  report it as `echo_seen`.
- `run_watch_waker` (`exec_tool/watch_wake.rs`) runs each wake as a spawned task
  (`OwnerWake.in_flight` / `due_after_flight`, results back on a channel), so a wake waiting up to 5 s
  for an orchestrator's echo holds up neither another orchestrator's wake nor the cue loop.
- The acceptance suites' stand-in agents echo what they are typed (`stty raw echo -echoctl`), and
  pin `echo_seen`; `an_orchestrator_that_never_echoes_does_not_hold_up_anothers_wake` added.

Doc: [architecture.md § The watch waker](../architecture.md#the-watch-waker-exec_toolwatch_wakers). Full record: [cross-package entry](../../../../docs/dev/changesets/2026-11-24-cursor-tui-submit-and-resume-model.md).

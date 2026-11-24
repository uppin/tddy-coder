# 2026-11-24 — A Cursor session on a Coder target is not verified end to end through a daemon

**Category:** Verification gap
**Source:** PR #1028 ([change history](../changesets/2026-11-24-cursor-tui-submit-and-resume-model.md)), live check on `remote-dev`.

PR #1028's two fixes were verified on `remote-dev` (Cursor Agent 2026.11.22, in tmux over
`coder ssh`) piece by piece, never as one daemon run:

- **Submit after the echo** — through `tddy-session-driver`, which runs the daemon's own target
  launch and the same `type_into`: single-line, multi-line and while-busy messages, each one
  `send_keys(text, [Enter])`, were all submitted with `echo_seen: true`.
- **Resume on the reported model** — three steps joined by hand:
  1. the driver's run, `/model` → GLM 5.3 Flash: the remote hook follower carried
     `model: glm-5p3-flash-high` on `beforeSubmitPrompt` and `stop`;
  2. those exact payloads, built into requests by `hook_request::session_status_request` (as
     `tddy-tools session-hook` does), through the real `ReportSessionStatus` handler: the record
     read `glm-5p3-flash-high`;
  3. the same chat relaunched by the driver with `--model glm-5p3-flash-high`: it opened on GLM 5.3
     Flash with its history, and its first message was submitted.

What no run has exercised is the **daemon's own wiring between those steps**: a `cursor-cli`
session started by a daemon on a Coder target; its remote hook follower delivering to that daemon's
`ReportSessionStatus`; then `ResumeSession` with a message, through
`resume_session_at_session_coordinate`, the record's `model` and `paste_and_submit_to`.

## What would close it

One live run on a daemon built from master after #1028, with a Coder account in the signed-in
subject's vault:

1. `StartSession` a `cursor-cli` session on the Coder workspace with `model: auto`;
2. `SendSessionMessage` a single-line and a multi-line message, and one while a turn runs — each
   answered `echo_seen: true` and submitted;
3. `/model` → another model in the session's terminal, then one turn;
4. `TerminateSession`, then `ResumeSession` with a message: the relaunch's argv carries the picked
   model (the `.session.yaml` `model` shows it first), the TUI's footer names it, and the message is
   submitted.

Record the outcome here and delete the entry; a step that fails is a defect of its own.

**Why deferred:** the daemon reaches a Coder workspace only through a signed-in subject whose
unlocked vault holds the Coder account — a GitHub sign-in and a vault unlock on a dev stack, which
an unattended agent cannot do. The pieces above cover each step's code; only the plumbing between
them is unobserved.

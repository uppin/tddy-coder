# 2026-11-15 — Muse Code lifecycle hooks over `-c hooks=…` (#911)

**Type:** Fix
**Packages:** `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-changeset`, `tddy-session-activity`, `tddy-session-lifecycle`, `tddy-tools`, `tddy-daemon-rpc`, `tddy-desktop`

Resolves the backlog entry `2026-11-14 — A Muse Code session reports no activity, question,
status or notification` (`2026-11-14-muse-hooks-not-wired.md`, deleted in this wrap). Narrows
`2026-11-14-muse-answer-session-question.md` to its Later items (its Now item is this change's
refusal) and `2026-10-22-muse-code-cli-facts-unconfirmed.md` past the `hooks: unsupported` line.

## Problem

A `muse-cli` session reported no activity, status, question or notification: while Muse waited in
its `request_user_input` selection UI, `ReadSessionActivity` answered empty,
`WaitProjectNotifications` delivered nothing, and `ListProjectSessions` showed the lifecycle
fallback. All of these are fed by lifecycle hooks running `tddy-tools session-hook`, and
`muse.yaml` declared `hooks: { adapter: unsupported }` with `capabilities: []`.

## Probe (2026-11-15, Muse Code 1.4.12, before any code change)

Scratch dirs under `/tmp`, a logging hook on every event via `.muse/hooks.json` and `-c hooks=…`,
the TUI driven under tmux (`~/.config/muse/settings.json` untouched):

- `PreToolUse` does **not** fire for `request_user_input` — before, during, or after the wait
  (it fires for other main-session tools). No hook can carry a blocking answer, and hooks never
  see question content or ids.
- Project `.muse/hooks.json` is **not** loaded before workspace trust; `-c hooks=…` (the file's
  inner map, no top-level key) installs hooks without a file and **merges** with
  `managed_hooks_path` hooks. `-c` is accepted after the positional prompt.
- `Elicitation`/`ElicitationResult` are refused as unsupported events; `muse exec` runs no hooks.
  `SessionEnd` delivery on exit is unconfirmed; `Stop` fires reliably.
- `Notification` while waiting: `{"notification_type":"question_prompt","title":"Muse Code needs
  your input","message":"<workspace> — waiting for your answer",…}` — no questions, no ids.
- `request_user_input` input (from `muse export`, never visible to hooks):
  `{"questions":[{"header":"Color","id":"favorite_color","options":[{"label":"red"},{"label":"blue"}],"question":"What is your favorite color?"}]}`
  — Claude's `AskUserQuestion` shape plus a per-question `id`, without `multiSelect`.

## Change

- `tddy-agent-backend`: `build_muse_hooks_config_value` emits Claude's eight events with
  byte-identical `session-hook` commands (both builders share `hook_entry_for_event`) as the `-c`
  value; `PreToolUse` matches `request_user_input`; every hook is report-only.
- `tddy-coding-agent-specs`: `muse-config-arg` adapter; `try_render` returns
  `RenderedHooks::File | Argv`, `delivery()` classifies them, `slots::render` splices argv renders
  into `argv_extra`. `muse.yaml` declares the adapter with `capabilities: [hooks]`, without
  `ask_user_question`.
- `tddy-agent-launch`: host starts render `-c` through slots; resumes render argv hooks again
  from the recorded token (`resumed_argv_hooks`, minting none); jailed starts install none.
- `tddy-changeset` / `tddy-session-activity` / `tddy-tools`: `question_prompt` →
  `WaitingForInput`; `request_user_input` alongside `AskUserQuestion` in the question check, the
  pending-question record, the "asks" text, and the tool-call exclusion.
- `tddy-session-activity`: notification texts name the session's agent product from its spec.
- `tddy-daemon-rpc`: `AnswerSessionQuestion` on an agent without `ask_user_question` is refused
  naming the product and `ClaimSessionTerminal` + `SendSessionKeys`.

Deliberately not done: answering through hooks (proven impossible), question content/ids in
`pending_question` (nothing carries them), jailed-Muse hooks (matches jailed Claude).

## Permanent docs updated

- [muse-cli-session.md](../../ft/daemon/muse-cli-session.md): Lifecycle hooks section, `-c` in the
  spawn argv, the `.muse/` tree confirmed.
- [telegram-notifications.md](../../ft/daemon/telegram-notifications.md): texts name `<product>`.
- `tddy-coding-agent-specs` `coding-agent-specs.md`: the adapter, `muse: [hooks]`,
  `MUSE_INSTALLED_EVENTS`.
- `tddy-agent-launch` `module-layout.md`: resume argv hooks, `resumed_argv_hooks`, the jail's
  argv exception.

## Verification

TDD; scoped gates all green: full `./test -p` for changeset, backend, specs, launch (243 lib),
activity, tools; lifecycle harness 10/10, session 23/23, bus 19/19, LiveKit 6/6; daemon-rpc answer
suites 2/2, 5/5, 7/7, 4/4 plus git 9/9; desktop 25/25 and 8/8; clippy `-D warnings` clean on all
eight Rust packages. Two local failures proven unrelated: a `/tmp` vs `/private/tmp` Claude
assertion (fails identically with the change stashed) and transient `No space left` errors during
a shared-box disk squeeze (green on re-run). Full-workspace verification left to CI.

## Code issues re-measured (brace-matched, `origin/master` `66bb45213` vs HEAD)

- `tddy-coding-agent-specs:complexity-slots-mod-render`: 69 → 66, better, not clean — the hooks
  arm is extracted; the MCP arm extraction still closes it. Stays open, partially fixed.
- `tddy-agent-launch:complexity-cli-agent-steps-resumed-session-wiring`: 80 → 82 — the
  `resumed_argv_hooks` call and the `hooks` field. Stays open, regressed.
- `tddy-agent-launch:complexity-opencode-cli-spawn-resume-resume-cli-harness-session`: 131 →
  136 — the resume argv wiring, mirroring the start path. Stays open, regressed.
- Every other open issue in the nine touched packages names code this change did not touch and
  was left alone. No issue was resolved, so none was deleted.

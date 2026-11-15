# Muse Code lifecycle hooks over `-c hooks=…`

**Type:** Fix
**Packages:** `tddy-agent-backend`, `tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-changeset`, `tddy-session-activity`, `tddy-session-lifecycle`, `tddy-tools`, `tddy-daemon-rpc`, `tddy-desktop`

## Problem

A `muse-cli` session reports no activity, status, question or notification to tddy: while Muse
waited in its `request_user_input` selection UI, `ReadSessionActivity` answered empty,
`WaitProjectNotifications` delivered nothing, and `ListProjectSessions` showed the lifecycle
fallback `agent_status: "active"` with `pending_elicitation: false`. All of these surfaces are fed
by the agent's lifecycle hooks running `tddy-tools session-hook`, and `muse.yaml` declared
`hooks: { adapter: unsupported }` with `capabilities: []`, so none were ever installed.

Full root cause: `../todo/2026-11-14-muse-hooks-not-wired.md`. The premise behind `unsupported`
was wrong — Muse Code 1.4.12 has a Claude-compatible hook system — but its shape had to be
probed before the fix, since the fix's shape depends on it.

## Probe (2026-11-15, no tddy change; full notes in the hooks TODO)

In scratch dirs under `/tmp`, a logging hook on every event, via `.muse/hooks.json` and via
`-c hooks=…`, with the interactive TUI driven under tmux (`~/.config/muse/settings.json`
untouched; managed-hook observation via `-c managed_hooks_path=<probe file>`):

- (a) `PreToolUse` does **not** fire for `request_user_input` — before, during, or after the
  wait. It fires for other main-session tools (`bash`) and subagent tools. A blocking hook
  answer is impossible, and hooks never see question content or ids.
- (b) Project `.muse/hooks.json` is **not** loaded before workspace trust: an untrusted
  workspace boots to the trust prompt. (`muse exec` parses the file but runs no hooks at all;
  hooks are interactive-TUI-only.)
- (c) `-c hooks=…` **installs hooks without a file**: the value is the file's inner map. It
  works in an untrusted workspace (with `--trust-workspace` to skip the prompt) and is accepted
  after the positional prompt, so the spec's `{ splice: slots }` needs no reorder.
- (d) `-c hooks=` **merges** with `managed_hooks_path` hooks — both fire. Superset's hooks
  keep running.
- `Elicitation`/`ElicitationResult` are refused as unsupported events; the other fourteen are
  accepted. `Notification` while waiting carries `notification_type: "question_prompt"` with
  only a title and message — no questions, no ids. `SessionEnd` delivery on exit is
  unconfirmed; `Stop` fires reliably.
- `request_user_input`'s input, confirmed from `muse export`: Claude's `AskUserQuestion` shape
  plus a per-question `id`, without `multiSelect`, option `description` optional.
- Hook payloads carry `model` (`"unknown"` under `--provider echo`), `model_provider`
  (`"meta"` on real-provider events), `permission_mode`, `cwd`; `transcript_path` was null in
  every observation. Subagent tools fire `Pre`/`PostToolUse` with the child session id.

The two payloads future work builds on, verbatim. The `Notification` while the selection UI
waits:

```json
{"notification_type":"question_prompt","title":"Muse Code needs your input","message":"<workspace> — waiting for your answer","hook_event_name":"Notification","session_id":"<main>","turn_id":"…","cwd":"…","transcript_path":null,"model":"muse-wonder-1.0","permission_mode":"default","model_provider":"meta"}
```

The `request_user_input` tool call, from `muse export` (never visible to hooks):

```json
{"questions":[{"header":"Color","id":"favorite_color","options":[{"label":"red"},{"label":"blue"}],"question":"What is your favorite color?"}]}
```

## Change

**Adapter: `muse-config-arg`** (`tddy-agent-backend`, `tddy-coding-agent-specs`).
`build_muse_hooks_config_value` emits Claude's eight events
(`build_claude_hooks_settings`' set) with byte-identical `session-hook` commands — both builders
now share `hook_entry_for_event` — as the `-c hooks=` value: the hooks map without the file's
top-level key, serialized compactly. `PreToolUse` matches `request_user_input`; every hook is
report-only (`--await-answer` nowhere), since no Muse hook of ours can carry an answer back.
Params without a daemon socket are refused (`NoDaemonSocket`), as Cursor's are.

`HooksAdapter::try_render` now returns `RenderedHooks::File | RenderedHooks::Argv`, and the
trait gains a `delivery()` classifier (`File` by default, `Argv` for Muse). `slots::render`
splices argv renders into `argv_extra` and writes no file for them. `muse.yaml` declares
`hooks: { adapter: muse-config-arg }` and `capabilities: [hooks]` — deliberately without
`ask_user_question`.

**Launch and resume** (`tddy-agent-launch`). Host starts render the `-c` entries through the
unchanged slots path. Resumes render argv hooks again from the recorded token
(`resumed_argv_hooks`; a resume mints none, and a session that recorded none resumes hookless,
as before); file adapters keep the start's file untouched. A jailed start renders no argv
hooks, so jailed Muse runs hookless — as a jailed Claude session does.

**Shared mapping** (`tddy-changeset`, `tddy-session-activity`, `tddy-tools`).
`notification_type: question_prompt` maps to `WaitingForInput`, and `request_user_input` stands
alongside `AskUserQuestion` in `hook_event_asks_a_question` (new `REQUEST_USER_INPUT_TOOL`
const), in the pending-question record (its input parses as Claude's: `id` ignored, missing
`multiSelect` reads single-choice), in the pending-question "asks" notification text, and in
the `session-hook` tool-call exclusion. A Muse wait therefore raises status and notifications;
its `pending_question` carries the `Notification` message alone, since nothing in the hook
stream carries the questions.

**Notification texts** (`tddy-session-activity`). `notification_for_activity_status` takes the
session's agent product from its spec (`CodingAgentSpec::product`) instead of the hardcoded
"Claude Code"; the hook report resolves it in the same lookup as its `hooks` gate, so an
unknown session type is refused exactly as before.

**Answer refusal** (`tddy-daemon-rpc`). `AnswerSessionQuestion` on a target whose agent
declares no `ask_user_question` is refused first — before the orchestrator and fit checks —
naming the product and pointing at `ClaimSessionTerminal` + `SendSessionKeys`. Sessions no
agent declares (a `tool` session's clarification) fall through to the existing checks, which
keep speaking for them.

What this does **not** do: answer a Muse question through hooks (proven impossible — see the
answer TODO's remaining Later items); fill `pending_question` questions/ids for Muse (nothing
carries them); install hooks for jailed Muse (matches jailed Claude); touch the `muse.yaml`
resume argv or key facts (still owned by `2026-10-22-muse-code-cli-facts-unconfirmed.md`).

## Evidence

TDD throughout: each behaviour got its failing test first (spec/adapter output for Muse, the
`question_prompt` and `request_user_input` mappings, the product in notification texts, the
answer refusal).

- `tddy-agent-backend`: `muse_hooks` builder tests (eight events, `request_user_input`
  matcher, identity in every command, nothing awaits an answer); the `claude_hooks` extraction
  is covered by its unchanged tests.
- `tddy-coding-agent-specs`: adapter resolution, `-c` argv shape (compact JSON, eight events,
  socket reporting), socket-less refusal, per-adapter delivery, installed events, and the
  `slots::render` argv split; the registry capabilities test now pins `muse: [hooks]`.
- `tddy-agent-launch` + `tddy-session-lifecycle`: `a_host_muse_start_and_resume_render_muse_yamls_argv`
  asserts `-c hooks=…` on the start and on the resume, the eight events, and the resume's reuse
  of the recorded token; the jailed muse argv test is unchanged (no `-c` in the jail).
- `tddy-changeset`, `tddy-session-activity`, `tddy-tools`: the `question_prompt` mapping, the
  `request_user_input` mapping/record/exclusion, and the product in the WaitingForInput/Done
  texts (Claude's telegram-copy tests pin its text unchanged).
- `tddy-daemon-rpc`: answering a muse-cli child is refused naming Muse Code and the two
  terminal tools.

Scoped verification (each package touched, never the workspace):

- `./test -p tddy-changeset -p tddy-agent-backend -p tddy-coding-agent-specs`: every target
  green, 0 failed. Specs re-run after the `render_hooks_slot` extraction: 20 targets green.
- `./test -p tddy-agent-launch`: 14 targets green (lib 243/243), re-run after the jail
  cleanup.
- `./test -p tddy-session-activity`: every target green, re-run after moving the new unit
  tests to the file end (clippy `items_after_test_module`).
- `./test -p tddy-tools`: every target green.
- `./test -p tddy-session-lifecycle --test harness_clis_from_spec_acceptance`: 10/10
  (start `-c`, resume `-c` with the recorded token, jail argv unchanged);
  `--test opencode_cli_session`: 23/23 (muse argv updated to the new contract);
  `--test session_notification_bus_unit`: 19/19 (Claude's telegram copy pinned, Muse's
  product asserted); `--test session_sync_livekit_acceptance`: 6/6 (needed
  `tddy-remote-git-repo` built first — a test dependency `./test` does not build).
- `./test -p tddy-daemon-rpc --test project_home_answer_question_acceptance`: 2/2 (the
  refusal plus the untouched happy path); `--test project_home_answer_permission_acceptance`:
  5/5; `--test cross_host_answer_wait_acceptance`: 7/7; `--test spawner_read_scope_acceptance`:
  4/4; `--test project_home_git_tools_acceptance`: 9/9.
- `./test -p tddy-desktop --test native_notifications_unit`: 25/25;
  `--test native_notifications_acceptance`: 8/8.
- `cargo clippy -p <each of the eight Rust packages> --all-targets -- -D warnings`: clean.
  `cargo fmt -p …`: applied.
- Full-workspace runs were left to CI per house rules. Two local failures were proven
  unrelated: `claude_cli_from_spec_acceptance/an_unsandboxed…` (`/tmp` vs `/private/tmp`
  under the nix shell — fails identically with this change stashed) and the LiveKit/git
  suites' `No space left on device` during a shared-box disk squeeze (green on re-run).

## Standing complexity records touched (all unclaimed, all deferred to the `#agent-tools` sweep node; the wrap re-measures)

- `packages/tddy-coding-agent-specs/docs/code-issues/complexity-slots-mod-render.md`: `render`
  shrinks — the hooks arm moved into `render_hooks_slot`, beside `render_tool_slot`.
- `packages/tddy-agent-launch/docs/code-issues/complexity-cli-agent-steps-resumed-session-wiring.md`:
  +2 lines (the `resumed_argv_hooks` call and the `hooks` field); the logic lives in the new
  helper, not the function.
- `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md`:
  +5 lines (the toolset take, the `hook_params` call, the slot field, a two-line comment),
  mirroring the start path's wiring.

## Documentation delta

- `docs/ft/daemon/muse-cli-session.md` should gain the hooks section at wrap: `-c hooks=…`
  delivery and why (trust, no committable file, managed-hooks merge), the eight events, the
  report-only rationale (probe (a)), and what a Muse wait does and does not record (status and
  notifications yes; question content/ids no; answers via terminal tools).
- TODO dispositions, done in this change: deleted
  `../todo/2026-11-14-muse-hooks-not-wired.md` (fixed; this changeset's probe section is the
  record, payloads included); narrowed
  `../todo/2026-11-14-muse-answer-session-question.md` to its Later items (its Now item is this
  change's refusal, the hook answer marked disproven); narrowed
  `../todo/2026-10-22-muse-code-cli-facts-unconfirmed.md` past the `hooks: unsupported` line
  (the four fact rows stay).

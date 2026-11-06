# 2026-11-05 — Codex's OAuth capture still hands the headless agent one `TDDY_*` variable

**Category:** Deferred from `#agent-tools` 15/19 (#821, headless-driver), to the Codex OAuth follow-up stack
**Source:** `packages/tddy-coding-agent-specs/specs/codex.yaml` (`coding_agent.headless.env`); `packages/tddy-agent-backend/src/backend/codex.rs` (`CODEX_OAUTH_AUTHORIZE_URL_FILENAME`, `spawn_codex_oauth_login`'s `BROWSER`/`TDDY_CODEX_OAUTH_OUT`)

The stack rule is that a headless agent's process sees no `TDDY_*` variable: the generic headless
backend strips every inherited one, and a `tddy-tools` the agent runs finds its session through
`.tddy/session.json` in its working directory instead.

Codex's OAuth capture is the one exception. A signed-out `codex exec` opens its login URL through
`BROWSER`, which the backend points at this process (`{self_exe}`), and the re-executed process
writes the URL where `TDDY_CODEX_OAUTH_OUT` names (`{session_dir}/codex_oauth_authorize.url`), so the
web UI can show it. `codex.yaml` declares both as templated `headless.env` values (green of #821),
marked `TODO(headless-driver)`.

To close it, the Codex OAuth follow-up stack moves the capture's destination out of the agent's
environment — for example a `BROWSER` argument (`{self_exe} --codex-oauth-out <path>`), or the
re-executed process finding the session through the same pointer — and deletes the variable from
`codex.yaml`. Until then, the acceptance test that pins "no `TDDY_*` variable" runs on Claude, not
Codex.

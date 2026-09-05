# 2026-07-21 — Agent-driven tool replacement: no-bash & no-write sandbox sessions

- A subagent def's `replaces:` can bind ANY exec tool; restrictions are declared on the agents config (no mode flags) and rendered into the in-jail CLAUDE.md/AGENTS.md appendix ([no-bash-mode.md](../no-bash-mode.md)).
- Replacing `Shell` makes the def the session's action author: Shell + the native Bash family are hard-disabled; commands run only through session actions (`request_action` — the author writes a bounded YAML manifest, the host re-validates and auto-establishes it — then `invoke_action`).
- Replacing `Write`/`StrReplace`/`Delete` makes a def the session's coder: native edit aliases hard-disabled; the subagent loop gains Managed-only `WRITE`/`STR_REPLACE`/`DELETE` tools (binding validated before spawn).
- Shared authored-manifest validation (`tddy_core::session_actions::validate_authored_manifest`); host-side `EstablishAction`/`ListActions`/`InvokeAction` handlers in `tddy-sandbox-app`; `Shell`/`Await` hard-rejected at the host relay when Shell is replaced.
- Follow-ups: Linux/daemon path, non-HTTP backend kinds as a def-schema extension, async invoke via `session_action_jobs`, repo-catalog promotion.

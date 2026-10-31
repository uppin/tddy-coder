# One coding-agent registry: agent enums and string gates become spec capabilities - PRD

**Date**: 2026-10-31
**PRD Type**: Technical Improvement (breaking configuration change)
**Stack**: `#agent-tools 12/12` (branch `feature/agent-tools/agent-registry`)

## Affected Features

- **Primary**: [Session agent roster](../session-agent-roster.md) and [Harness profiles](../harness-profiles.md). The agents a host offers are read from the spec registry.
- **Related**: every `*-cli-session.md`, daemon configuration (`config.example.yaml`, `dev.daemon.yaml`) and `tddy-coder --agent`.

## Summary

Code no longer branches on an agent's name. It reads the agent's spec:

- **Ten or more parallel enumerations collapse into the `tddy-coding-agent-specs` registry.** Among them are `AnyBackend`, `Harness` (for the provider route), `BUILTIN_BACKEND_AGENT_IDS`, the two backend-construction matches, `backend_selection_question` and the model and context-glob string tables.
- **The `session_type == "claude-cli"` gates become declared capabilities.** The gates are in hook reports, question answers, operator notifications, notify, session deletion, branch intent, project-home, split and requires-model. They become `capabilities: [hooks, ask_user_question, split_host, project_home_main, requires_model, …]` on the agent spec.
- **Provider-route compatibility** (which harness takes which provider, and how) becomes spec data: `route: { providers: [anthropic, openai_compatible, meta], shape: env|args|config_file }`.
- **Breaking:** daemon.yaml `claude_cli:`, `cursor_cli:` and `cli_harnesses:`, and the `TDDY_*_CLI` and `TDDY_*_BINARY` env overrides that are not named in a spec's `binary.env`, are removed.
  - The daemon refuses to start with a removed key, naming its replacement: `<tddyhome>/tools/<id>.yaml`.
  - The non-binary keys move as follows: `claude_home_dir` to `home.key`, `fullscreen` to `run.env`, and `answer_wait_secs` to a `coding_agent` field.
- **The agent catalog exposes each agent's spec metadata:** kind, harness and route support. That closes the tool-session provider picker's remaining question ([2026-10-22-tool-session-provider-picker-needs-harness-metadata.md](../../../dev/todo/2026-10-22-tool-session-provider-picker-needs-harness-metadata.md)).

## Acceptance Criteria

- [ ] A new agent is added by dropping a YAML file into `<tddyhome>/tools/` that names existing adapters. It appears in the host's agent list, starts as a host PTY session, starts in a jail, and offers the capabilities it declares. No Rust is changed.
- [ ] Removing `hooks` from an override of `claude.yaml` turns off hook-based activity for that agent. This proves the gate is the capability, not the name.
- [ ] A daemon.yaml with `claude_cli:` fails start-up with a message naming `<tddyhome>/tools/claude.yaml`.
- [ ] `tddy-coder --agent <id>` accepts any registry id. Its interactive selection lists the registry.
- [ ] The agent catalog reports kind, harness and route support per agent, and the web picker stops asking for routes for definition agents.

## Out of Scope

- A UI for editing specs.
- Remote spec distribution.

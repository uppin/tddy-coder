# `start_cli_harness_session` 397 ➜ 371 lines

**Date:** 2026-11-19
**Type:** Refactor
**Packages:** tddy-agent-launch

Four steps of the generic CLI start became private helpers beside it in `src/opencode_cli_spawn.rs`,
cut by `tddy-tools restructure` (`extract_method`, no hand-written moved code):

- `log_hooks_on_the_target`: on a target, the hooks append to the hook log there.
- `place_slot_files_and_pointer`: writes the slot files and places the session pointer, through the
  target's shell or on the ssh exec host.
- `node_launcher_for`: no node launcher on a target.
- `report_the_started_session`: the started log line and the response.

The target-shell resolution stays inline. The engine refused it because the signature names
`CoderCredentialSource`, which the file does not import. The record
`docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md` is narrowed (371,
budget 60).

`tests/acme_target_support` now holds the `acme` spec, the host agent, the hook sink, `InstalledHooks`
and `ADaemon`, which `coder_agent_launch_acceptance` and `remote_hooks_acceptance` had carried as
identical copies.

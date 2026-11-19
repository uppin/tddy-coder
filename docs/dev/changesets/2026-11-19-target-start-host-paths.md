# 2026-11-19 — A target start names no host path, a failed Coder command says why, and a target cut has a real base

**Type:** Fix · **Packages:** `tddy-agent-launch`, `tddy-remote-shell` · Backlog:
[`docs/dev/todo/2026-11-19-coder-remote-agent-local-paths.md`](../todo/2026-11-19-coder-remote-agent-local-paths.md)
(narrowed, not closed)

## Problem (live on `mantasi/remote-dev`, start `start-1795066655480-49443-2`)

A Claude start on a Coder workspace (Linux, `HOME=/home/coder`) failed at its last call:
`mkdir -p '/Users/mantasi/.tddy/sessions/<id>' && cat > '…/agent-system-prompt.md'`, because
`launch_on_target` wrote every argv file of the daemon host's session directory to **the same host
path** on the target (since #917). The error and the start record said only `coder ssh`'s wrapper
("Process exited with status 1"): the pty had merged `mkdir`'s stderr into stdout, and
`CoderShell::output_of` kept the CLI's stderr as the command's. The worktree itself was cut from the
literal `'HEAD'` — the workspace clone's local `master`, which `git fetch --all` never moves.

## What changed

- **`remote_agent_launch::write_rendered_session_files`** takes and returns the plan: an argv entry
  naming an existing, non-slot file under `RemoteLaunch.session_dir` is written owner-only at
  `<worktree>/.tddy/<path relative to session_dir>` on the target and the argument rewritten to it.
  Argv only; an env value naming a session file is still not mapped (backlog L8).
- **`CoderShell::output_of`**: the marker on stdout with a non-zero status is
  `ExecOutput::exited(status, merged words)` — stderr is the command's merged output, stdout empty,
  the CLI's wrapper dropped. Status 0 is unchanged. Two existing coder acceptance tests that pinned
  `stderr: []` for a failed command now pin the merged words as stderr.
- **`cli_session_worktree::remote_base_ref`** (new): a `new_branch_from_base` cut on a target or an
  ssh exec host takes the explicit or chain base, else resolves the project's default integration
  base from this host's checkout (`tddy_core::resolve_default_integration_base_ref`, bounded by the
  spawn timeout); a resolution failure fails the start with its reason. A work-on-selected-branch
  start resolves nothing. `remote_worktree_through` takes `Option<String>` and no longer defaults to
  `"HEAD"`; a new branch reaching it with no base is an internal error.

## Tests

- `remote_agent_launch_acceptance::the_agent_on_a_target_reads_its_appended_system_prompt_from_the_target`
  (the `acme` fixture spec gained `{ splice: slots }`).
- `coder_shell_acceptance::a_failed_write_through_a_merging_pty_says_why_the_command_failed`, with
  the stand-in `a_coder_that_reports_a_failed_command_as_the_real_cli_does` (merges, and prints
  `CODER_SSH_FAILURE_WRAPPER` on a non-zero exit as v2.33.5 does).
- `cli_session_worktree::remote_cut_tests::a_target_start_naming_no_base_is_cut_from_the_projects_fetched_main_branch`.

## Not in this change

The operator-notify wiring on a target (the unreachable `TDDY_DAEMON_SOCKET` in `.tddy/session.json`
and the notify guidance in the prompt), a recipe on a target (not refused), the route config env
value, the usage context's `agent_home`, and a daemon-log line for a failed start remain in the
backlog entry above.

# Initial discovery: the `#coder-start` final sweep (10/10)

**Date:** 2026-11-19 · base `c965b89a7` (#949) · stack merge base with `origin/master` `8b004d090`

## The backlog, collected

`git diff --name-status 8b004d090..c965b89a7 -- docs/dev/todo/ 'packages/*/docs/code-issues/'`:

| Item | State at the base | Kind |
|---|---|---|
| `tddy-agent-launch` `complexity-opencode-cli-spawn-start-cli-harness-session.md` | **regressed by the stack**: 383 → 397 (brace-matched on `8b004d090` and `c965b89a7`); the record dates from #797 (2026-11-03) | code issue |
| `tddy-agent-launch` `complexity-remote-agent-launch-launch-on-target.md` | added by #944 (this stack's first sweep), length closed there; 6 parameters > 5 remain | code issue |
| `tddy-tool-engine` `complexity-start-session-args-schema-schema-offering.md` | confirmed unchanged (192) by #944; not worsened | code issue, out of scope |
| `docs/dev/todo/2026-11-16-remote-shell-test-fixtures-are-duplicated.md` | narrowed by #949 | TODO |
| `docs/dev/todo/2026-11-16-coder-hosts-never-run-against-a-real-deployment.md` | narrowed by #949: hook follower and non-running status words unverified | TODO |
| `docs/dev/todo/2026-11-18-idle-session-reported-as-waiting-on-a-permission.md` | partly fixed by #946; two read oddities remain, not reproduced | TODO |
| design question from #948's `[F]` stage | a target start needs the agent on the daemon host | new TODO (product decision) |

No `TODO`/`FIXME` marker was added by the stack: `git diff -U0 8b004d090..c965b89a7 | grep '^+.*\b(TODO|FIXME)\b'` is empty. The three markers in files the stack touched (`cursor.yaml:144`, `project_home/turn.rs:92`, `hook_reports.rs:4`) predate it.

## `start_cli_harness_session`

One straight body, 397 lines, of which the `CliSessionStart` destructure alone is 44. The stack's own
growth is three spots (+14): the `recorded_by` wrap of the target shell (#948), the conversation's
`ConversationTarget` (#947), and the target-aware node launcher (#947).

Ranges whose inputs are five or fewer locals (so an extraction adds no parameter-count finding):

| Range | Inputs | Lines saved |
|---|---|---|
| the started log and the response | kind, session id, pid, worktree, OS user | ~13 |
| the hook log on a target | target, wiring | ~8 |
| the slot files and the pointer | slots, target shell, remote host, wiring, worktree | ~10 |
| the node launcher | target shell, agent | ~4 |
| the target shell resolution | target, subject, credentials | ~7 |

Every other step (the metadata, the LiveKit exposure, the slot rendering, the launch itself) reads
seven to twenty locals; extracting it would trade the length finding for a parameter-count one. A
length under 60 needs parameter structs over the start's inputs, which is a design change, not a
restructure. #924 recorded that ranges naming `Option<Arc<dyn RemoteShell>>` are refused while the
file does not import `RemoteShell`.

## Test fixtures

A line-for-line hash of top-level items across the target-session suites found copies that are
**identical**:

- `tddy-session-lifecycle`: `derived_host_start_acceptance` / `orchestrator_target_start_acceptance` —
  `AnOperatorHome` and its `HostUserFiles` impl, `a_stub_agent`, `a_host_service`,
  `ADaemonReachingAnSshHost`, `a_daemon_reaching_an_ssh_host` (differing only in the project id
  constant), and four of the five methods.
- `tddy-agent-launch`: `coder_agent_launch_acceptance` / `remote_hooks_acceptance` — `the_acme_spec`,
  `the_acme_agent`, `TheSessionsActivity` with its `HookSink` impl, `InstalledHooks`, `ADaemon`,
  `a_daemon`.
- `tddy-session-lifecycle`: `coder_target_session_acceptance` / `remote_target_session_acceptance` —
  `AnAgentRun` and its impl (15 lines; the two files share no support module and their stub agents differ).

Everything else the TODO lists has drifted (the stub-agent families differ in their run line, the
trap and the banner; agent-launch's `ATarget`s keep different call logs).

## Coder deployment

`coder.ireland-devex.wixprod.net`, read-only: `GET /api/v2/workspaces?q=owner:me` with
`Coder-Session-Token` (the request `HttpCoderWorkspaceSource` makes) answered 200, with `running` for
`mantasi/remote-dev` and `stopped` for `bazel`, `qape-hq` and `remote-big`. No workspace was started
or stopped.

# 2026-11-16 — Coder hosts were never run against a real Coder deployment or the real `coder` CLI

**Category:** Future enhancement
**Source:** `#remote-shell` stack (#915–#921), recorded at the wrap of #921 (7/7)

Nothing in this stack was run against a real Coder deployment or the real `coder` CLI.
Four facts are pinned only by stand-ins — a stand-in `coder` executable, stand-in credential
and workspace sources, and in-memory web backends:

| Fact | Where it is assumed | Assumed |
|---|---|---|
| Workspace argument form | `CoderShell` (`tddy-remote-shell`) and the launch tests' stand-in `coder` | `coder ssh <owner>/<workspace>[.<agent>]` — the `owner/workspace.agent` form |
| Deployment variables | the shell's child environment | the deployment URL in `CODER_URL` and the token in `CODER_SESSION_TOKEN` |
| CLI probe output | the host tooling probe | what `coder version` prints on a daemon with the CLI installed |
| Workspace status values | `CoderWorkspace::is_running` and the derived-host reason | the deployment reports `running` for a usable workspace; every other word (`stopped`, `starting`, …) is carried verbatim into `the workspace is <status>` |

**Narrowed at the wrap of #927 (`#remote-shell` 11/11):** nodes 9–11 of the stack were
planned from a real-deployment run's findings, and their listings, checkout root and
orchestrator start are pinned by the same stand-ins above. What still has no
real-deployment evidence is the hook follower on a Coder workspace; the four facts remain
unverified against a real deployment.

**Narrowed 2026-11-18 (Coder one-shots cross the pty intact):** `CoderShell` was run against the
real deployment `coder.ireland-devex.wixprod.net` (CLI v2.33.5, workspace `mantasi/remote-dev`).
The first two facts hold: `coder ssh --disable-autostart mantasi/remote-dev` reaches the workspace
in the `owner/workspace` form (the `.agent` suffix was not exercised), and the deployment URL and
token are taken from `CODER_URL` / `CODER_SESSION_TOKEN`. The same run measured what the CLI does to
a command (always a pty: no stdin EOF, `onlcr`, merged stderr, about 6–8 s per call), now pinned by
stand-ins. The CLI probe output, the workspace status values and the hook follower are still
unverified against a real deployment.

**Why deferred:** no real deployment was available to the stack; the stand-ins pin the
contract the code was built against, not the deployment's actual answers.

**What would close it:** run the Coder hosts flow once against a real deployment with the
real `coder` CLI on the daemon — connect an account, list its workspaces, start a session on
a running workspace, and try a stopped one — correct whichever of the four facts differs,
and update the stand-ins and the feature docs to match.

**Narrowed 2026-11-18 (`#coder-start` 9/10, #949):** `tddy-session-driver/tests/coder_live_production.rs`
runs against `mantasi/remote-dev` and passes: `coder version` prints `Coder v2.33.5+81139b5 …`, a
usable workspace reports `running`, and Claude and Cursor start there and reach their prompts. Still
unverified: the hook follower on a workspace (needs a daemon-level start — the driver never creates
`hooks.log`), and the status words of a non-running workspace (none was tried).

**Narrowed 2026-11-19 (`#coder-start` 10/10, the sweep):** the status words were read from the real
deployment, read-only. `GET /api/v2/workspaces?q=owner:me` with `Coder-Session-Token` (the request
`HttpCoderWorkspaceSource` makes) answered 200, with `latest_build.status` `running` for
`mantasi/remote-dev` and `stopped` for the stopped `bazel`, `qape-hq` and `remote-big`, which is
the word `CoderWorkspace::is_running` refuses and `the workspace is stopped` carries. No workspace was
started or stopped, so the transient words (`starting`, `stopping`, `failed`) were not seen. They
are carried verbatim, so none needs code. **Still unverified:** the hook follower on a Coder
workspace. That needs a daemon-level start on a live workspace (the driver never creates
`hooks.log`), checking that its `SessionStart` reaches the session's status.

**Narrowed 2026-11-19 (session-driver hooks):** the hook follower ran on a Coder workspace.
`tddy-session-driver`, which wires and follows a session's hooks exactly as a daemon target start
does (token, `remote_log` hook slots, `create_remote_hook_log`, `RemoteHookFollower` through
`CoderShell`, `ActivityHookSink`), drove Claude and Cursor on `mantasi/remote-dev`: every hook
arrived in order with the daemon's reports (Claude `SessionStart` → `Started`, `UserPromptSubmit` →
`Running`, `PostToolUse` → `ExecutingTool` + activity `Bash`, `Stop` → `Done`; Cursor's lower-case
events likewise). The same run found, and the change fixed, a `tail -f` the follower left on the
workspace after each session. **Still unverified:** the daemon's own delivery of those reports into
its activity service on a live workspace — the step the driver replaces with a record — that is,
a daemon-level start whose `SessionStart` shows in the session's status.

**Narrowed 2026-11-21 (`#target-parity` 1/7, #969):** the daemon's own delivery is evidenced. A
daemon-level start of Claude on `mantasi/remote-dev` (the probe that planned `#target-parity`) had
its `SessionStart`, `UserPromptSubmit`, `PostToolUse`, `Notification` and `Stop` followed from
`.tddy/hooks.log` through `CoderShell` into the daemon's activity service, and the session's
status followed them. The hook follower on a Coder workspace is no longer unverified. **Still
unverified:** the answer channel #969 adds — the waiting `PermissionRequest` hook polling
`.tddy/answers/<nonce>` on the workspace and the daemon writing that file through `CoderShell`
(temporary file, then `mv`) within the hook's `<wait + 5>` seconds. It is pinned by a stand-in
target running the real hook commands under `sh`
(`tddy-agent-launch/tests/remote_hooks_acceptance.rs`), not by a live workspace. **What would
close it:** answer one permission prompt of a target session on `mantasi/remote-dev` with
`AnswerSessionPermission` and see the agent proceed.

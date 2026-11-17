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

**Why deferred:** no real deployment was available to the stack; the stand-ins pin the
contract the code was built against, not the deployment's actual answers.

**What would close it:** run the Coder hosts flow once against a real deployment with the
real `coder` CLI on the daemon — connect an account, list its workspaces, start a session on
a running workspace, and try a stopped one — correct whichever of the four facts differs,
and update the stand-ins and the feature docs to match.

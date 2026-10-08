# Changeset: Git and GitHub operations resolve the project's account

**Date**: 2026-09-19
**Status**: 🚧 In Progress · ✅ **#492 is this stack's base** — see Prerequisites
**Type**: Architecture Change
**Stack**: `#keyring` 9/9 · branch `feature/keyring/github-identity` · base `feature/keyring/link-github` (#515)

## Affected Packages

- **tddy-github** (**after #492**): the REST client takes a resolved token instead of reading the environment
- **tddy-accounts** (new in `#keyring` 4/9): the resolver is called here for the first time in anger
- **tddy-daemon-livekit**: [livekit-service.md](../../../packages/tddy-daemon-livekit/docs/livekit-service.md)
  - `src/session_room.rs` — the session's git environment
- **tddy-daemon-auth**: [auth-service.md](../../../packages/tddy-daemon-auth/docs/auth-service.md)
  - `FileGitHubTokenStore`'s remaining readers retired
- **tddy-tools**: the PR tools ask the session's host for the project account's token per call
  (`src/github_credential.rs`) — see *Token delivery to the agent's tools*
- **tddy-toolcall**: the `github-token` verb on the session toolcall socket
  (`GithubCredentialHandler`, per-instance, like `ChildSpawnHandler`)
- **tddy-session-lifecycle**: `SessionGithubCredential` answers it from `acting_identity`; one
  `SessionAccountAccess::session_identity` lookup now binds both the commit pairs and the handler on every
  session spawn path this daemon owns (see *M2 — session-start wiring*)
- **tddy-toolcall**: also the client half — `request_github_token` / `request_github_token_from_session`,
  shared by `tddy-tools` and the workflow actions
- **tddy-daemon-rpc**: `RepointPlannedPr` resolves the project's account itself (`project_github_token`)
- **tddy-workflow-recipes** / **tddy-pr-stack**: the engine-driven stack tasks ask the session host for the
  token **only when an action that reaches GitHub runs**; the prompt awareness gate reads a context flag
  instead of a hardcoded `false`
- **tddy-github**: `RealGithubPrApi::asking` / `asking_the_session_host` — a client that asks for its token the
  first time an operation needs one
- **tddy-host-service**: `HostSessionService` gains the `GithubToken` method and a `HostSessionRegistry`
  (session id → handlers); see *Tool sessions: the token over the per-OS-user host-session socket*
- **tddy-coder**: `conversation_spawn_relay.rs` (`HostSessionClient`, `DaemonRelayGithubCredential`) and
  `tool_host_wiring.rs`; its toolcall listener binds a `GithubCredentialHandler` exactly when it was given
  `--host-session-socket`
- **tddy-presenter** / **tddy-workflow**: the run's context is seeded with `github_pr_tools_available`
  (`context_keys::GITHUB_PR_TOOLS_AVAILABLE_KEY`, moved down so the presenter and the recipes name one string)
- **tddy-github**: `RealGithubPrApi::with_api_base` (the REST root is a value on the client, defaulting to
  `https://api.github.com`, never an environment variable) and the `curl_github_*_with_token` entry points take it
- **tddy-telegram-control**: a Telegram-started session whose project acts as an account tells its chat that the
  account's identity and tools are unavailable there, and why — see *Telegram-started sessions*
- **tddy-spawn**: the commit-identity pairs ride the supervisor/worker wire (`SpawnRequest::git_environment`,
  `SpawnOptions::git_environment`) — see *Tool sessions: the commit identity over the spawn wire*

## Related Feature Documentation

- [PRD — Git and GitHub operations resolve the project's account](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-github-identity.md)
- [Cross-daemon session authentication](../../ft/daemon/session-auth.md)
- [Session rooms](../../ft/daemon/session-room.md)
- [Project concept](../../ft/daemon/project-concept.md)

## Summary

Every git and GitHub operation the daemon performs for a session uses **the account the project is
assigned** — token *and* commit identity, from one resolution.

This is the node the stack exists for. 3/9 put credentials somewhere safe, 4/9 made them visible, 5/9
let a project name one, 8/9 allowed a second to exist; none of it changes a commit's author until
here.

## Background

Today a token comes from the process environment — `github_rest_common.rs:17`, *"`GITHUB_TOKEN`
preferred, then `GH_TOKEN`"* — so every project a daemon serves acts as the same GitHub user. A
commit identity comes from the checkout's `user.email`, which on a daemon host is whatever the
machine was configured with.

One deliberate exception stays: `session_room.rs:337` signs WIP snapshots under a fixed machine
identity because *"this object is not the agent's work, it is a machine-made snapshot of it"*. That
reasoning is unaffected by a project having an account.

## Responsibility

**This node owns the resolution from project to acting GitHub identity, and the removal of every
other way one could be chosen.**

- calling 5/9's resolver at the session's edge, once, for both the token and the identity;
- a distinct failure for each `AccountResolution` outcome;
- deleting the environment-variable resolution path;
- retiring `FileGitHubTokenStore`'s remaining readers.

## Boundaries

**Owned surface:**

| Symbol | Package |
|---|---|
| the token parameter replacing environment resolution | `tddy-github` (after #492) |
| the session git environment built from one resolution | `tddy-daemon-livekit` |
| the four resolution outcomes' failure messages | `tddy-accounts` |

**Explicitly not this node's:**

- **The REST client's move** — #492's, entirely. This node neither moves a file nor claims the record.
- **The resolver** (5/9) — called, not changed.
- **Linking** (8/9) — a second account already exists by the time this node runs.
- **WIP snapshot identity** — kept as it is, with the code's own reason.
- **A credential helper** — considered, deferred, and recorded below.

**The line this node must not cross**: **no environment fallback.** Not "prefer the account, fall
back to `GITHUB_TOKEN`", not "use the environment when nothing is assigned". A fallback here means a
push that should have failed instead succeeds **as the wrong person**, to a repository that person
can write to.

## Dependencies

**Parent in the line**: `#keyring` 8/9 `link-github` — [#515](https://github.com/uppin/tddy-coder/pull/515).
**A real edge**, and the only in-stack one that also matters semantically: resolving *a* project's
account is only meaningful when more than one account can exist.

**The load-bearing edge is 5/9** [#512](https://github.com/uppin/tddy-coder/pull/512) — the
assignments and `AccountResolution`. 3/9 and 4/9 are transitive through it.

**#492 is an ancestor. ⚠ It is not yet a *green* one.** On 2026-09-19 the whole `#keyring` stack
was re-based onto [#492](https://github.com/uppin/tddy-coder/pull/492) `feature/carve/git-plumbing`,
which removes the merge-order collision this node was planned around. What the plan then asserted —
that "the moved `tddy-github` is present in this tree from node 1 onward" — **is false today**, and
was false when it was written: #492 is itself in its wave-2 red state. Measured on this branch on
2026-09-20:

- `packages/tddy-git` does not exist;
- `packages/tddy-github/src/` is `auth_service.rs`, `lib.rs`, `provider.rs`, `real.rs`,
  `session_token.rs`, `session_token_v2.rs`, `stub.rs`, `token_store.rs` — no `github_pr`, no
  `github_rest_common`, no PR surface of any kind;
- `packages/tddy-github/tests/git_plumbing_shape.rs` is #492's **failing** acceptance suite, and its
  four assertions are among this branch's inherited red;
- all 2,124 lines of the REST client, and all ten callers of `github_token_from_env`, are still in
  `tddy-workflow-recipes`.

**Measured again on 2026-09-20, after the stack-wide rebase: the move has landed.** `#carve` pushed
`#carve 6/11` — *"tddy-git is born, and GitHub REST moves to the crate named after the service"* —
onto `feature/carve/git-plumbing`, and the cascade brought every layer of this stack onto it. At this
branch's tip `packages/tddy-git/src/` holds `lib.rs` and `ssh_exec.rs`, and the same 2,124 lines now
sit in `packages/tddy-github/src/` as `github_pr.rs` (494), `github_rest_common.rs` (338) and
`pr_api.rs` (1,292). `github_token_from_env` is **defined** at `github_rest_common.rs:19`, with 14
call sites across five files.

So the post-#492 module paths this node's contract was written against are **real on this branch
now**. What that does not change: #492 is still red, and still moving — it was `5/9` when this node
was planned and is `6/11` today. The REST half of the contract was published as deferred before the
base moved, and the deferral stands with a better reason than the one it was written with: green does
that work against paths that exist, rather than wave 2 doing it against paths that were about to be
rewritten.

The consequence is scheduling, not design. Nothing about *where* the token parameter belongs
changed.

**Dependents**: none. This is the top of the stack.

**New external dependencies: none.**

## Draft PR contract

Published in this PR's **second commit**:

**Surface**

- `tddy-github`'s REST entry points taking a token parameter (**signatures only**, and written against
  the post-#492 module paths);
- the session git-environment constructor's signature in `tddy-daemon-livekit`.

**Failing tests**

- a commit made in a session carries the assigned account's name and email;
- a GitHub API call uses the assigned account's token;
- two projects on one daemon, assigned different accounts, act as different GitHub users;
- token and identity always come from **one** resolution — a test that fails if they are resolved
  independently;
- `NotAssigned` fails GitHub operations with that reason, and **`GITHUB_TOKEN` in the daemon's
  environment does not rescue it**;
- `UnknownOnThisHost` fails with *that* reason, distinct from `NotAssigned`;
- `Ambiguous` fails rather than picking;
- WIP snapshot commits are still authored by `tddy-daemon`.

⚠ **Not mergeable in that state** — implementation follows in this same PR.

### ⚠ Plan correction — the REST half could not be published

The contract's first surface bullet promised `tddy-github`'s REST entry points taking a token
parameter, *"written against the post-#492 module paths"*. **When the surface was published those
paths did not exist**: #492 was an ancestor in its own wave-2 red state, the client was still 2,124
lines in `tddy-workflow-recipes`, and `packages/tddy-git` had not been created. The
`## Dependencies` claim that the move *"has already happened in this tree"* was corrected above.

**Hours later the base moved and the move landed** — see the 2026-09-20 measurement under
`## Dependencies`. The paths are real on this branch now. That vindicates the deferral rather than
reversing it: had the parameter been published against the pre-move files, every one of them would
have been rewritten underneath it by `#carve 6/11`.

**What was done instead.** Publishing the parameter against the pre-move paths was rejected: it means
editing the exact files #492 is moving, which is what `## Boundaries` puts out of scope (*"The REST
client's move — #492's, entirely"*) and what basing the stack on #492 existed to avoid. So the REST
half is **deferred to this node's green phase**, which now runs against real paths. Two
consequences, both recorded rather than hidden:

- acceptance criterion *"a GitHub API call uses the assigned account's token"* is covered in wave 2
  only as far as `ActingIdentity::token` — the assertion at the REST boundary arrives with M1;
- no test in this commit constrains where the parameter lands. M1 is unchanged in scope; it simply
  has no red test in front of it yet.

**Nothing about the design changed.** The token still comes from one resolution, and the
no-environment-fallback rule is pinned by
`a_project_that_assigns_no_account_is_refused_although_the_environment_holds_a_token`, which sets
`GITHUB_TOKEN` in the process and asserts the refusal anyway.

### As published — measured red (wave 2, commit 2)

Scoped to this node's own verification scope — `./test -p tddy-accounts -p tddy-github
-p tddy-daemon-livekit -p tddy-daemon-auth --no-fail-fast`, run on 2026-09-20 before and after the
surface commit. **Scoped, not whole-workspace**; CI is the authority on everything else.

| | passed | failed |
|---|---|---|
| Baseline, before this commit | 279 | 76 |
| After publishing the surface | 280 | 95 |

**+19 failing, +1 passing, 0 previously-failing tests fixed or hidden.** The 76 inherited failures
are nodes 2–8's unimplemented green phases plus #492's own four `git_plumbing_shape.rs` assertions;
every one of them is still failing, by name, for the same reason.

Where the 19 land, and what each fails on:

| Suite | New red | Fails at |
|---|---|---|
| `tddy-accounts/tests/project_resolved_identity_acceptance.rs` | 5 | `identity.rs` — `acting_identity` |
| `tddy-accounts/tests/acting_identity_unit.rs` | 8 | `identity.rs` — `acting_identity` (5) and `IdentityError: Display` (3) |
| `tddy-daemon-livekit/tests/session_git_identity_acceptance.rs` | 3 | `session_git.rs` — `session_git_environment` |
| `tddy-daemon-auth/tests/login_time_token_store_is_retired.rs` | 3 | their own assertions — the readers are still there |

Sixteen of the nineteen panic on one of this node's three `todo!()`s and on nothing else; the other
three are the structural assertions about `FileGitHubTokenStore`'s remaining readers, which fail
because those readers exist.

**One test is green from the moment it was written**, and is recorded rather than manufactured into
red: `a_work_in_progress_snapshot_is_still_signed_by_the_daemon_itself`. It guards what this node
promised **not** to change, so red would mean the promise was already broken. Proven non-vacuous by
mutation — renaming `WIP_COMMIT_IDENTITY_NAME` to a person's login fails it, and restoring the
constant passes it again.

### Design decisions taken while publishing the surface

1. **One entry point, `acting_identity`, returning one `ActingIdentity` carrying both halves.** The
   alternative — a `token_for` beside an `identity_for` — is what the contract bullet *"token and
   identity always come from one resolution"* rules out, and a shape no test can fully police. There
   is nothing to police here: the crate publishes no way to obtain one without the other.
2. **The git identity is derived from the record's provider metadata, never from its label.**
   `META_SUBJECT_ID` and `META_SUBJECT` (`#keyring` 8/9) give `101+ada@users.noreply.github.com` and
   `ada`; `CredentialRecord::label` is the one mutable field and is deliberately not identity-bearing
   — `renaming_an_account_does_not_change_who_its_commits_are_authored_by` pins that.
3. **A fifth refusal, `IdentityError::Unusable`, beyond the four `AccountResolution` outcomes.** A
   record can resolve and still carry no provider identifiers. Refusing it — naming the absent
   metadata key — is the alternative to inventing an address, and a commit authored under an invented
   address is attributed to nobody and discovered only by reading history.
4. **`session_git_environment` returns pairs rather than applying them to a `Command`.** The caller
   that spawns the agent is the only thing that knows which process they belong on, and a list of
   pairs is something a test can read without running `git`.
5. **`tddy-daemon-livekit` gained a path dependency on `tddy-accounts`.** No external dependency, and
   no cycle: `tddy-accounts` names `tddy-credentials`, `tddy-rpc` and `tddy-service`, none of which
   reach back. The direction is the point — the crate deciding which account a project acts as knows
   nothing about sessions or LiveKit.

### `GITHUB_TOKEN` inside a session — settled: it goes too

The surface was published with this left open, because `github_token_from_env` is defined in
`tddy-github/src/github_rest_common.rs` and called from 14 sites across five files, and some of them
run inside the *agent's* process, where the daemon setting the variable is plausibly a delivery
mechanism rather than a fallback. **The developer settled it on 2026-09-20: delete it entirely.**

So the environment read is not narrowed to the daemon and left standing in the agent — it stops
existing, and a token reaches a REST call only by being passed to it. What that costs, and what M1
therefore owes:

- `tddy-github` — the definition at `github_rest_common.rs:19` and its ten uses across
  `github_rest_common.rs`, `github_pr.rs` and `pr_api.rs`. These are the entry points M1 was already
  threading a token through; the deletion is the same edit finished rather than a second one.
- `tddy-workflow-recipes` — the re-export in `lib.rs` and the caller in `merge_pr/github.rs`. This is
  the part the earlier hedge was about: `merge_pr` runs where the agent runs, so it gains a token
  parameter of its own and its caller must resolve one. **Green must not restore the environment
  read under another name** — a `token: Option<String>` that falls back when `None` is the same
  fallback with a longer path, and CLAUDE.md forbids it without consent that has not been given.
- `tddy-workflow-recipes/tests/github_pr_acceptance.rs` — asserts the variable is absent before
  exercising the unauthenticated path. It is rewritten against whatever replaces the parameter, not
  deleted wholesale, because the behaviour it pins (no credential ⇒ refuse) is still the behaviour.

Pinned by `no_crate_still_resolves_a_github_token_from_the_process_environment` in
`packages/tddy-daemon-auth/tests/login_time_token_store_is_retired.rs` — a structural test, because
"nothing reaches for this any more" is not a question the type system answers while the function
still exists.

## Green-phase findings

**Done.** `acting_identity` (one `resolve_account`, then the record's own token and an identity derived
from `META_SUBJECT_ID` / `META_SUBJECT`), `IdentityError: Display` (one message per variant, each naming
the remedy), `session_git_environment` (four `GIT_*` pairs from one `ActingIdentity`, token excluded).
`github_token_from_env`, `github_env_token_present`, `TokenSource::ProcessEnv` and the env-reading
`curl_github_{get,post,patch,put}_json` wrappers are **deleted**; `create_pull_request`,
`update_pull_request`, their `*_via_rest_api` forms and `merge_open_pr_for_branch` take `token: &str`
and refuse a blank one.

**Not done at the time — refused rather than given a fallback.** No resolved account reached these callers, and
the process environment is no longer a credential, so each refused. **Since closed** (see *Closing the
remaining seams* below):

- ~~`tddy-tools` — the two PR MCP tools (`github_token_for_agent`) and `real_gh` / `pr_search_impl`~~ —
  **done**, see *Token delivery to the agent's tools*;
- ~~`tddy-workflow-recipes` — `orchestrate_pr_stack/actions.rs` (merge and repoint tasks)~~ — **done**;
- ~~`tddy-pr-stack` — `assess.rs`~~ — **done**;
- ~~`tddy-daemon-rpc` — `RepointPlannedPr` (`pr_stack/ports.rs`)~~ — **done**, resolved directly (not through
  `retained_github_token`, which is the login-keyed credential this stack retires);
- ~~prompt awareness of the PR tools (`merged_red_system_prompt(false)`, merge-pr hooks)~~ — the gate now reads
  a context flag — **since closed**: the coder sets it when it binds the host-session handlers (*Tool sessions: the token over the per-OS-user host-session socket*).

**M2's wiring is the open item.** Something must read the project's assignments and the session's
vault, call `acting_identity`, and apply `session_git_environment` to the spawned agent. That call site
lives in the session-spawn path (`tddy-session-lifecycle` / `runtime::build`), is not in this node's
tests, and was not written. Delivering the *token* to the agent is a separate decision: the
four-variable test deliberately excludes it.

## M2 — session-start wiring

**Wired**: the co-located (unsandboxed) claude-cli session, on **start** and on **resume**. One
`acting_identity` resolution per start or resume — the project's `accounts` assignments against the
GitHub records of the caller's own vault (`SessionVaultAccountStore::list(session_token)`) — whose
commit identity goes through `session_git_environment` into the agent's `env_extra`. Code:
`tddy-session-lifecycle/src/connection_service/session_acting_identity.rs`; the token is **not**
delivered, and nothing in the path reads `GITHUB_TOKEN` / `GH_TOKEN`. `tddy-session-lifecycle`
gained path dependencies on `tddy-accounts` and `tddy-credentials` (no external crates, no cycle).

### Decision (developer-consented): a refused resolution does not stop the session

When resolution fails for **any** reason — `NotAssigned`, `UnknownOnThisHost`, `Ambiguous`,
`Unusable`, a vault that is locked / uninitialized / has no such session / is unavailable, or no
vaults configured — the session **still starts, with no `GIT_*` variables added**, and the refusal is
logged at `warn` (`tddy_daemon::connection_service`).

⚠ **This is the one place the identity half is not strictly enforced.** On a refused project the
agent's commits use the checkout's inherited `user.name` / `user.email` — the status quo — so a
commit can still be authored by whoever the machine is configured as. It is accepted because it keeps
existing, unassigned projects working. It is *not* a token fallback: no credential is delivered on
any path, so a push still cannot succeed as the wrong GitHub account through this seam.

### Follow-ups

Closed — see *Closing the remaining seams*. The tool-session (`tddy-coder`) commit identity is closed too (*Tool sessions: the commit identity over the spawn wire*); its token delivery is closed too (*Tool sessions: the token over the per-OS-user host-session socket*).

## Token delivery to the agent's tools

**Decision (developer, explicit): the token is asked for, never delivered.** `tddy-tools` sends a
`github-token` request over the session's own toolcall socket (`TDDY_SOCKET`) at the moment a PR tool
runs; the daemon resolves the session's project account and answers that one call. The token is in no
environment variable and no file, on any path.

**How the session is identified.** The toolcall socket is bound **per session** and carries its
handlers per instance (as `spawn-child` and `spawn-conversation` already do), so the socket *is* the
identity — the request names no session, project or account, and a tool can only ask about its own.

**How the daemon obtains the token.** `SessionGithubCredential` (in
`tddy-session-lifecycle/src/connection_service/session_acting_identity.rs`) is built at session start
or resume from the project's `accounts` assignments and the *start's session token*. Per call it reads
the owner's vault through `SessionVaultAccountStore::list(session_token)` and runs the same
`acting_identity` the commit identity came from — one resolution function, `ActingIdentity::token`
returned and kept nowhere. The daemon recovers the owner's vault from the session token held by the
handler (`SessionSubjectResolver` maps it to the subject, `SessionVaults` opens by subject); nothing is
read from session metadata and nothing was missing.

**Consequences, stated rather than hidden**

- **The account is chosen once, at start/resume, and the handler is pinned to that outcome** (see *One resolution,
  pinned at the session's start*): a reassignment, or a second account appearing, takes effect on the next start or
  resume. The vault is read **per call** (for the pinned account's record, by id), so a vault locked since, or an
  owner whose session token expired, refuses at the call.
- The handler holds the start's session token in daemon memory for the session's life. When it
  expires, tools refuse with `NoSuchSession` until the session is resumed with a live token.
- **Refusals are distinct and verbatim.** `NotAssigned`, `UnknownOnThisHost`, `Ambiguous`,
  `Unusable` (the `IdentityError` messages) and the vault refusals (`NoSuchSession`, `Locked`,
  `Uninitialized`, `Unavailable`, no vaults configured) travel as `{"status":"error","message":…}`
  and the PR tool returns `{"error": <that message>}` to the agent — not a generic authentication
  failure. No `TDDY_SOCKET` is also an error naming it; there is no environment fallback anywhere.
- The token is kept out of logs: `ToolCallResponse::GithubTokenOk` holds a `RedactedToken` (its `Debug`
  is fixed text) and the listener's `[send]` log line withholds it.
- The daemon's own `GITHUB_TOKEN` / `GH_TOKEN` changes nothing — pinned in the lifecycle unit tests and
  in `tddy-tools`.
- No proto change: `github-token` is a JSON verb on the existing toolcall service, so
  `scripts/generated-code.sh` has nothing to regenerate and the `unbundle_service_split` closed-world
  RPC list is untouched.

**Alternatives considered**

- *A secret file in the jail.* Rejected: a file is on disk and readable by the agent and everything in
  its jail; it outlives the call; rotation and revocation need cleanup; the developer ruled out any
  on-disk token.
- *An environment variable (`GITHUB_TOKEN` on the agent's process).* Rejected: visible to every
  process in the session and printable by the agent — exactly the exposure this changeset's earlier
  *Decision recorded with its alternative* flagged — and it reintroduces the variable this node deleted.
- *A credential helper for `git`.* Still deferred: it does not cover the REST calls the tools make.

**Where it is wired**: every session spawn path this daemon owns, listed in *Closing the remaining seams*.

**Still not exercised or not done**

- an in-jail `tddy-tools` that cannot reach the host listener was **not exercised**: the jail path is
  verified by the shared code path and the doc comment above, not by a running jail (the sandbox RPC
  bridge acceptance suite is unavailable here);
- ~~the *token's use at GitHub* is not exercised~~ — **closed**, see *The REST call carries the host's token*;
- tool-session (`tddy-coder`) token delivery and the engine-driven prompt flag — closed, see *Tool sessions: the token over
  the per-OS-user host-session socket*.

## Closing the remaining seams

The follow-ups the two sections above left behind. **One lookup binds both halves**:
`SessionAccountAccess::session_identity(session_id, Option<&[AccountAssignment]>)` returns a
`SessionIdentity { git_environment, github_credential }` from the project's assignments read once by the
caller; `LaunchHost::session_identity(os_user, session_id, project_id, session_token)` is the host-side
wrapper (project lookup, then this). It replaces `LaunchHost::session_github_credential`. `None` assignments
(no project row — a client-supplied `repo_path`, or an unreadable project, the latter logged) yields neither
half; a project that does not resolve yields no pairs but **still binds a handler**, which refuses each
request with the resolver's own message.

| Path | Commit pairs | Token handler | Where the pairs go |
|---|---|---|---|
| co-located claude-cli start / resume | yes (earlier) | yes (earlier) | the agent's `env_extra` |
| sandboxed claude-cli **start** | **yes (new — it had the handler only)** | yes (earlier) | `session_env`, below |
| sandboxed claude-cli **relaunch** (resume) | yes (new) | yes (new) | `session_env` |
| sandboxed cursor-cli start | yes (new) | yes (new) | `session_env` |
| plain cursor-cli start / resume | yes (new) | none — a cursor-cli session runs no toolcall listener | the cursor process env |
| agent-spawned children (`spawn-child`, `spawn-conversation`) | yes (new) | yes (new, via the child's own start) | as the claude-cli start |
| **tool session (`tddy-coder`) start / resume** | yes | **yes — over the per-OS-user host-session socket**, see below | the child's process environment, over the spawn wire |
| split-agent | **not applicable** | none | — |

**Why the jail's pairs go in `session_env`.** The jailed agent never mounts the checkout; it edits and
commits through the host-side Shell relay, whose commands run under the env `dial_and_bridge` is given
(`session_env`). Putting the pairs in the runner's own env would reach a process with no repository. The
sandboxed claude-cli *start* did **not** carry the pairs before this change; it had the token handler only.

**Split-agent: `GIT_*` does not apply.** The agent's working directory is a context dir, not a checkout, and
every commit is made by the tools running on the codebase daemon; this process's environment never reaches
them. The split resume path therefore drops the pairs it used to add (inert, and noisy when the project
could not be read) and keeps the handler binding it already had.

**Agent-spawned children.** `StackChildSpawnHandler` and `GrillMeConversationSpawnHandler` now hold the
orchestrator's `SessionAccountAccess` (vaults, subject resolver, **the orchestrator's start token**) and hand
it to the child's `spawn_claude_cli_session_inner` instead of `SessionAccountAccess::none()`. The trust
decision, stated: a child is a session of **the same owner on the same project** (`os_user` and `project_id`
are the handler's own), so it acts as the account that project assigns, resolved over the owner's vault the
orchestrator was already reading. Nothing crosses a trust boundary: the token stays in daemon memory, and the
child gets pairs and (via its own handler) per-call tokens, never the credential or the session token. The
consequence is the one the orchestrator already has: the handlers hold the orchestrator's start token, so a
child spawned after it expires is refused with `NoSuchSession` and starts under the checkout's identity (the
consented refusal). A child of a *different* project is not a case — the handlers are bound to one.
The conversation handler is also threaded through the tool-session (`tddy-coder`) host socket, so a
conversation a grill-me tool session spawns resolves the same way.

**Daemon-side `RepointPlannedPr`.** Resolves directly: the orchestrator's project from its
`.session.yaml`, its `accounts`, and `project_github_token(vaults, user_resolver, caller's token, accounts)`
— one `acting_identity`. Only a node that **owns a branch** needs GitHub (it has a PR to re-target); a
plan-only repoint reaches no GitHub call, so it is never refused and never asks the vault
(`token_for_repoint`). A refusal is `FAILED_PRECONDITION` with the resolver's words, **before** the plan is
rewritten, so a refused repoint changes nothing.

**Agent-process workflow actions** (`AssessTask`, `MergeTask`, `RepointTask`) ask the host with the same
`github-token` round trip `tddy-tools` makes (`tddy_core::toolcall::request_github_token_from_session()`,
reading `TDDY_SOCKET` only to learn *whom to ask*). The socket client lives in `tddy-toolcall`
(`request_github_token`); it could not sit in `tddy-tools`, which depends on `tddy-workflow-recipes`. ⚠ These
tasks belong to `OrchestratePrStackRecipe`, which is **retained but inert in production** (`recipe_resolve` maps
every CLI name to `PrStackRecipe`); they have no live driver. The live callers of the same helpers
(`assemble_views`, `execute_stack_merge`, `execute_stack_repoint`) are the `pr_*` tools, which already pass a
token.

**They ask on demand, never before.** *Decision (developer):* a task asks the host only when an action that
reaches GitHub actually runs, so plan-only paths never touch the socket and a refusal surfaces at the action that
needed the token, in the host's own words. Measured against the code, the premise "asks when constructed" did not
hold — `build_graph` and the `*Task::new()` constructors never asked; the eagerness was at the top of each task's
`run`. What changed:

| Task | Before | Now |
|---|---|---|
| construction (`build_graph`, `*Task::new()`) | asked nothing | asks nothing (pinned) |
| `AssessTask` | asked on every run | `RealGithubPrApi::asking_the_session_host`: asks the first time a node that owns a branch has its PR looked up; a stack whose nodes own none asks nothing. A refusal propagates through `assemble_views`'s `?` |
| `RepointTask` | asked on every run | asks only when `repoint_reaches_github` — some dependent owns a branch, so there is a PR to re-target. Asked **up front** in that case, not lazily per call, because `execute_stack_repoint` downgrades a failed GitHub call to a `warn` and a lazily-surfaced refusal would be swallowed while the dependents' PRs kept their old base |
| `MergeTask` | asked at the top of `run` | unchanged: merging always reaches GitHub, and asking *before* `execute_stack_merge` is what keeps a refusal from leaving a `Planned` journal behind (pinned: no `stack-op.json` after a refused merge) |

`RealGithubPrApi::asking` holds a *supplier*, not a token: a refusal is **not remembered** (the next operation asks
again, so assigning an account mid-run is not stuck behind the earlier refusal) and a token the host did hand over
is kept for that client's life, as a `with_token` client keeps its own. The sync supplier runs the async request on
a thread and runtime of its own, so it is safe from an async task and from a runtime thread and never depends on the
caller's runtime making progress while the caller waits. Pinned in
`tddy-workflow-recipes/tests/orchestrate_pr_stack_github_on_demand.rs` (against a real socket host that counts the
requests it receives) and the `real_impl_tests` in `tddy-github/src/pr_api.rs`.

**Prompt awareness.** `merged_red_system_prompt` and the merge-pr system prompts advertise the PR tools only
when the workflow context carries `github_pr_tools_available = true`
(`tddy_workflow_recipes::github_pr_tools`). Neither hardcodes `false` any more, and neither probes the
environment.

## Tool sessions: the commit identity over the spawn wire

A `tddy-coder` tool session is started by `ToolSpawnPlan` → `SpawnOptions` → one of three backends (the in-process
`spawn_as_user`, the forked `spawn_worker` over a JSON pipe, `tddy-supervisor` over its own wire). It now carries
**the four `GIT_AUTHOR_*` / `GIT_COMMITTER_*` pairs** of the account its project acts as, on **start and resume**:
one `SessionAccountAccess::session_identity` lookup (the same one every other path makes — `DaemonSessionHost::session_identity`),
of which only `git_environment` is used. A refused resolution still starts the session with no pairs, logged
(the consented decision). Pinned end to end in `tddy-session-lifecycle/tests/tool_session_git_identity.rs`, which drives
the real `StartSession` / `ResumeSession` into a script that records the environment it started with.

**Wire change.** `SpawnRequest` (the worker's JSON request) gains `git_environment: Vec<(String, String)>` with
`#[serde(default)]`; `SpawnOptions` gains `git_environment: &[(String, String)]` (`Default` = empty, so every existing
`..Default::default()` literal is unaffected); `SessionChildPlan` gains `env`, applied by `spawn_as_user`
(`Command::envs`) and, on the supervised path, added to the `SpawnSession` request's `env`
(`supervisor_spawn::with_commit_identity`). The supervisor's own `SpawnSessionRequest` **already had** an `env` map —
no supervisor protocol change.

**Compatibility.** The daemon, its forked worker and the supervisor are separate processes that may be at different
versions. *New daemon → old worker:* serde ignores the unknown field, the child is spawned without the pairs — the
session starts under the checkout's own identity (the same outcome as a refused resolution). *Old daemon → new
worker:* the field is absent, which deserializes as empty (`a_spawn_request_from_a_client_that_predates_the_commit_identity_carries_none`
removes the key from a real request and decodes it). The round trip is pinned
(`a_spawn_request_round_trips_its_commit_identity_over_the_worker_pipe`).

**What carries a token: nothing here.** The field's contract is the four pairs. `plan_session_child` refuses any other
key (`SESSION_GIT_ENVIRONMENT_KEYS`) before any I/O — naming the key, never the value — because the supervisor wire's
`env` is a generic map and a future caller could otherwise put a credential on it
(`a_github_token_is_refused_as_session_environment`, `a_loader_variable_is_refused_as_session_environment`). The
account's token reaches a session's tools only by being *asked for*, per call, over the toolcall socket
(`github-token` → `SessionGithubCredential`), on every path that has a handler; `a_planned_child_is_given_exactly_the_four_commit_identity_pairs`
and `the_accounts_token_is_in_no_variable_the_tool_session_starts_with` pin the environment, and `GITHUB_TOKEN` in the
daemon's environment resolves no identity (`a_github_token_in_the_daemons_environment_gives_an_unassigned_project_no_identity`).
A *daemon* that itself has `GITHUB_TOKEN` exported used to hand it to its children by ordinary process-environment
inheritance; `spawn_as_user` now removes both `GITHUB_TOKEN` and `GH_TOKEN` (see *Inherited environment* below).

**⚠ Operator consequence, supervised hosts.** The supervisor's `spawn_policy.allowed_env_keys` is a *deny*, not a
filter: a request naming a key it does not list is refused outright. A project that resolves to an account now sends
the four keys, so on a supervised host they must be listed (`supervisor.yaml.production`, `dev.supervisor.yaml` and
`daemon.yaml.production` now say so, in comments) or **every tool-session spawn for such a project is refused**, not
degraded. That fails loudly rather than silently starting under the machine's identity, which is deliberate; it is
also a behaviour change an upgrade must be told about. A project that resolves to nothing sends no key and spawns
exactly as before.

## Tool sessions: the token over the per-OS-user host-session socket

**Decisions (developer, explicit).**

1. **`--host-session-socket` is decoupled from recipes.** Every tool session the daemon starts or resumes gets it,
   whichever recipe (it used to be bound for grill-me only, and never on resume). What stays recipe-specific is the
   *handler*: only a grill-me session registers a `spawn_conversation` handler, so any other recipe's request is refused
   (`this session's recipe does not spawn conversations`) rather than silently spawning. `tddy-coder` tolerates the flag
   for every recipe; nothing keys off its presence to mean grill-me.
2. **One long-lived socket per OS user, not per session or per spawn.** Bound lazily by the first session that needs it,
   kept for the daemon's lifetime, bound again by the next session after a restart. Re-used by every tool call and every
   tool session of that user.
3. **`HostSessionService` gains `github_token`**, answered by the same `SessionGithubCredential` the claude-cli paths bind
   (`github_credential_handler`), built per tool session at start/resume from `session_identity`.

**Path scheme.** `<data dir>/run/<os user>/host.sock` — `tddy-session-lifecycle/src/connection_service/host_session_socket.rs`
(`host_session_socket_path`). The data dir is the daemon's configured one (what `sessions_base_for_user` resolves). `run/`
is `0o711` (traversable, not listable); `<os user>/` is `0o700`; `host.sock` is `0o600`.

**Permission model.** The directory is narrowed to `0o700` **before** the socket is created in it, so whatever mode the
socket is born with nobody else can reach it — there is no window with wider permissions. Ownership moves last (socket,
then directory) to the session's OS user when the daemon runs as someone else. A `chown` the daemon is not privileged to
make **refuses the bind and removes what it created** instead of widening anything (the rule `write_agent_def_file`
follows); the session then starts without the flag, logged. A stale file left by a dead daemon is replaced only after
checking it is a socket, owned by this daemon or the target user, and that **nothing is listening** on it; a live socket
is never taken over and a non-socket file is never removed. The `TODO(stdio-relay)` on the `0o777` is gone with it.

**Authentication and trust — stated plainly.** The socket's **filesystem permissions are the authentication boundary.**
Because one socket serves many sessions, **every request names its session** (`session_id`), but that id is **a label, not
a secret**: nothing proves the caller *is* that session. The daemon looks the id up in the registry, refuses an unknown or
deleted one (`not_found`), refuses one registered for a different OS user than the socket's owner (`permission_denied`,
naming neither user, no token), and otherwise answers from **that session's** handler — its assignment snapshot and the
token it was started with — so session A's token is never returned for session B's id. **The residual trust:** any process
running as the socket's OS user can ask for the token of any of *that user's* registered sessions. That is the same
boundary as the user's own vault (an agent already runs as that user), and it is why the directory is owner-only. Not
defended: a hostile process of the same OS user.

**Proto.** None, so nothing to regenerate and nothing for the `unbundle_service_split` closed-world list. `HostSessionService`
is JSON over `tddy-stdio` (`tddy.host.HostSessionService`, constants in `tddy-toolcall`), not a `.proto` service; the new
method is `GithubToken` (`GITHUB_TOKEN_METHOD`) beside `SpawnConversation`. **Additive:** a host that predates a method
answers `unimplemented` (pinned: `an_unknown_method_is_unimplemented_so_an_older_host_is_additive`). A request now carries
`session_id`; an old coder that does not send one is refused (`a_request_that_names_no_session_is_refused`) — coder and
daemon are one release, and the previous per-session socket is gone.

**Registry.** `HostSessionRegistry` (`tddy-host-service`): session id → `RegisteredSession { os_user, conversation handler,
github credential handler }`. `HostSessionSockets` (`tddy-session-lifecycle`) holds the registry and a map of per-user
servers; `ensure_bound` reuses a server whose task is alive and whose socket file is still the inode it bound, and otherwise
stops it and binds again. A session registers at start **and again at resume** (replacing the entry with the refreshed
assignments and token) and is unregistered at `DeleteSession`.

**How the coder reaches the token.** The coder already knows its session id (`--session-id`, or `--resume-from`, which
`run.rs` copies into `args.session_id`), so no new flag. `tool_host_wiring::ToolHostHandlers::from_flags(socket, session_id)`
binds `DaemonRelayConversationSpawnHandler` and `DaemonRelayGithubCredential` on the coder's toolcall listener
(`start_toolcall_listener_with_handlers`) **exactly when both are present**; absent socket → no handler → the existing *no
credential handler* refusal, no fallback. `HostSessionClient` connects on first use and again after the connection is lost
(a daemon restart), repeating only an idempotent request (the token), never a spawn. `tddy-tools` (child of the coder, via
`TDDY_SOCKET`) and the workflow actions' `request_github_token_from_session()` therefore work as in a claude-cli session.

**The prompt flag.** The same fact sets `github_pr_tools_available`: `run.rs` calls
`presenter.set_github_pr_tools_available(tool_host.github_pr_tools_available())`, the presenter seeds it into every run's
context, and the recipes' hooks read it. The key moved to `tddy-workflow::context_keys` so seeder and reader name one string.
Both `TODO(keyring 9/9)` markers (`tool_session_spawn.rs`, `github_pr_tools.rs`) are removed.

**Inherited environment.** Verified, not assumed: a daemon with `GITHUB_TOKEN` / `GH_TOKEN` exported **did** hand them to
a spawned tool-session child by ordinary inheritance (`spawn_as_user` does not `env_clear`) — the test below failed with both
lines present before the fix. `spawn_as_user` now `env_remove`s both keys, and the forked worker goes through the same
function. **Not covered:** the supervisor path starts the child from the *supervisor's* environment plus the request's `env`
(which `plan_session_child` already restricts to the four commit-identity keys); the supervisor's own environment is the
operator's configuration and is not touched here.

### Resolved in this PR

The deferrals this section used to list were worked in this PR; what each became is recorded below, and what did
**not** close is in *Still open*.

#### One resolution, pinned at the session's start

`SessionAccountAccess::session_identity` now resolves **once** (`acting_identity`) and both halves derive from that
outcome: the `GIT_*` pairs from the `ActingIdentity` (`session_git_environment`), and `SessionGithubCredential`
**pinned** to it. If the start resolved an account the handler holds that account's **id**; every later `github-token`
request lists the owner's vault (per call, never cached) and fetches **that** record by id — through the same
`acting_identity` function, over a one-entry assignment naming the pinned account — so it can never answer with another
account's token, whatever the project's assignments or the vault's contents become. If the start was **refused**
(the session started without `GIT_*` by the consented decision) the handler holds the reason and **repeats it verbatim**
on every request, until a resume resolves again: a session whose commits are not attributed to an account is never
handed that account's token. The consequence, stated plainly: a start refused only because the vault was *locked*
stays refused after the person unlocks it, until the session is resumed.

Pinned by (`session_acting_identity_tests.rs`): `a_start_refused_because_the_vault_was_locked_keeps_refusing_after_it_is_unlocked`,
`a_start_refused_for_an_unheld_account_keeps_refusing_once_the_account_arrives`,
`a_start_refused_for_no_assignment_is_not_rescued_by_the_environment_or_a_later_assignment`,
`the_token_stays_the_started_accounts_when_a_second_account_appears`,
`the_started_accounts_token_is_never_replaced_by_another_accounts_when_it_is_removed`,
`the_commit_pairs_and_the_token_derive_from_one_record`, and `each_call_reads_the_vault_as_it_stands` (rewritten: the
original constructed a handler over a *locked* vault and expected it to recover on unlock — exactly the behaviour this
decision removes; it now ends the token owner's session between two calls instead). The first two were red against the
previous code (`Ok("ghp_…")` where a refusal was required); the others pass on both and guard the construction.
`SessionGithubCredential::new`, `git_environment` and `git_environment_or_inherited` are gone; `git_environment_for`
survives under `#[cfg(test)]` as the pure helper the unit tests drive.

#### Stopped sessions and daemon restarts

- **Stop hook.** The daemon does not track a spawned `tddy-coder`, but the spawn result carries its pid. After a start
  or resume spawns, `HostSessionSockets::watch_until_stopped(session_id, pid)` records the pid on the registration and
  watches it (`kill(pid, 0)` every two seconds by default; `ESRCH` is gone, `EPERM` — a process of another user — is
  still there). When it is gone the session is unregistered, so the daemon stops holding the start's session token for
  a session that is not running. A resume registers again, which forgets the earlier pid, so an **old** process stopping
  late never removes the newer registration (`unregister_stopped` is conditional on the pid). The watch ends with the
  session's deletion or replacement. A spawn that fails unregisters. Polling rather than a process-exit event because
  the supervised backend's children are not the daemon's to `wait` on; a pid reused inside one poll interval would
  keep a dead session registered a little longer (memory only). Tests: `a_tool_session_whose_process_has_stopped_is_no_longer_answered`,
  `a_resumed_session_stays_answered_when_its_earlier_process_stops`, host-service `a_stopped_process_unregisters_its_session_only_while_it_is_still_the_registered_one`
  and `a_session_registered_with_no_process_is_not_removed_by_a_stopped_report`. Dropping the daemon's sockets now
  aborts its servers (`BoundServer: Drop`), so a daemon that starts over in one process finds no live peer.
- **Restart.** The registry is daemon memory and **no token is persisted**. An unregistered session that **exists on
  disk** (`.session.yaml` under the socket owner's sessions base, id validated as one path segment) is refused with its
  own message — `this session was started before the daemon restarted; resume it to re-enable GitHub tools`
  (`failed_precondition`) — distinct from an unknown or deleted one (`not_found`, *no session `<id>` is registered…*,
  which does not suggest resuming). The probe is asked about the socket's owner only, so a request can never learn that
  another OS user's session exists. Tests: host-service `a_session_that_exists_but_is_not_registered_is_told_to_resume_it`,
  `a_session_that_does_not_exist_is_not_told_to_resume`, `the_existence_of_another_os_users_session_is_not_revealed`;
  integration `after_a_daemon_restart_a_running_session_is_told_to_resume_and_then_is_answered` (a second daemon over the
  same data dir; the running session gets the message over the rebound socket, and the real token after a resume) and
  `after_a_daemon_restart_a_session_that_never_existed_is_not_told_to_resume`. The coder side (`HostSessionClient`)
  already relays the host's status text verbatim and reconnects (`the_connection_is_made_again_after_the_host_restarts_and_rebinds`).
- **No startup re-attach exists**, and none was added: re-registering needs the owner's session token, which is not durable
  by design. **What a running coder sees after a restart, until a session of that OS user is started or resumed:** the
  socket is bound only by that, so a connect fails and the coder reports *the daemon's host-session socket … could not be
  reached* — now followed by *if the daemon restarted since this session started, resume the session to re-enable GitHub
  tools* (`an_unreachable_socket_is_an_error_that_names_it`). Not a hang. The distinct message above is what it sees once
  the socket is rebound. (The vaults in the restart test stay open across the "restart"; what the test models is the loss
  of the registry, not of the vaults.)

#### The REST call carries the host's token

`RealGithubPrApi` gained `with_api_base(base)` (default `https://api.github.com`, a value on the client, **not** an
environment variable); the `curl_github_{get,post,patch,put}_json_with_token` and `…_absolute_path` entry points take the
base as their first argument (their only callers are `RealGithubPrApi`). `workflow-recipes/tests/github_rest_call_carries_the_session_hosts_token.rs`
runs a fake session host on a real toolcall socket (`TDDY_SOCKET`) and a listener on loopback standing in for GitHub:
`a_rest_call_carries_the_token_the_session_host_returned` asserts the request's `Authorization: Bearer <token>` is the one
`request_github_token_from_session()` returned; `a_refusing_host_means_no_request_reaches_the_api` asserts zero HTTP requests
and the host's words in the failure; `a_github_token_in_the_environment_authenticates_no_call`;
`the_token_travels_in_the_header_alone_not_in_the_url`. Non-vacuous by mutation (a wrong header token fails two). Proven for
a GET; the PUT/POST/PATCH path builds its header with the same format string but has no request-level test of its own.

#### Telegram-started sessions

**Investigated.** A Telegram start has an OS user (a daemon-config field) and a Telegram-to-GitHub-**login** link
(`TelegramGithubMappingStore`), but **no session token**; the vault opens only by an owner's session token. Reaching an
already-open vault *by login* would let a Telegram chat bypass that gate, so **no mechanism is invented**: such sessions
start under the checkout's own identity and **no token is ever available to them**. What changed is that this is now
**visible**: when the project assigns an account, the session's chat is told — after the start message, on the workflow,
claude-cli and cursor-cli paths — that the account's commits and the agent's GitHub tools are unavailable and why, and that
starting from the web dashboard acts as the account; the same text is logged at `warn`. A project assigning no account is
told nothing (nothing is lost). Tests: `a_telegram_started_session_on_a_project_that_acts_as_an_account_says_the_account_is_unavailable`
(red first), `a_telegram_started_session_on_a_project_with_no_account_says_nothing_about_accounts`
(`tddy-telegram-control/tests/telegram_start_claude_acceptance.rs`; the claude-cli path is the one exercised end to end,
the workflow and cursor paths call the same method). `TODO(keyring 9/9)` is **removed**. **`TODO(stdio-relay)` remains**
(`workflow_spawn.rs`), reworded: a Telegram-started *tool* session gets no host-session socket, because the socket answers
from a registration the daemon's own session host makes and this crate does not hold it; without one the socket would only
refuse. Telegram never wired `spawn_conversation`, so it is not a regression.

### Still open

- **A daemon that cannot `chown` to the session's OS user gets no socket for that user — NOT FIXED, blocked on a design
  decision** (D3). That is the documented production topology: `tddy-supervisor` (root) runs `tddy-daemon` as an
  **unprivileged** child and sessions run as other users, so the daemon cannot give a socket to them; its sessions start
  with no `--host-session-socket` — no token and no `spawn_conversation`, which worked over the old `0o777` socket. The
  per-session precedent does not transfer: `agent_tool_socket` (embedded daemon) applies **no** permissions at all, and
  `write_agent_def_file` is written by the *spawning* process, which is the privileged one under the supervisor. The two
  supervisor precedents that do exist are (a) the supervisor binds a service socket **as root** and hands it to the
  daemon as fd 3 with a `group`/`mode` grant, declared per service in `supervisor.yaml` — one socket per service, not per
  OS user, and fd passing is not part of the spawn request surface; and (b) `SpawnSession` carries a path/`env` the
  supervisor already gates by `spawn_policy`. A directory the supervisor creates *user-owned 0700* (the shape the brief
  suggested) would be unusable: the unprivileged daemon could not bind inside it. Every safe shape needs the root
  supervisor to perform a **new privileged filesystem operation on a path the daemon names** (e.g. a `SpawnSession`
  field asking it to `fchown` a daemon-created `0600` socket to the session user, gated by `allowed_session_users` and a
  socket-owned-by-caller check), or per-OS-user sockets declared in `supervisor.yaml`. Either widens what a compromised
  daemon can ask root for — the line `supervisor.yaml` calls *the ENTIRE privilege surface of the host* — and is the
  maintainer's decision, so nothing was added. What *was* done: the refusal now says exactly this (*not privileged … as
  when it runs as tddy-supervisor's unprivileged child*) and still removes everything it created; no wider mode is ever
  used. **Who can connect today:** a daemon able to `chown` (root, or the session user itself): only that OS user and the
  daemon (`0700` directory, `0600` socket, both owned by the user; root can always connect). A daemon that cannot: nobody —
  there is no socket.
- **The socket path lives under the data dir** (`<data dir>/run/<os user>/host.sock`), not under the session's own
  directory: that nests too deep for AF_UNIX's ~104-byte limit on a real checkout path (the overflow
  `agent_tool_socket_path` was digested to avoid). A data dir deep enough that the path exceeds 100 bytes is refused
  with that reason.
- **A running tool session is not re-registered after a daemon restart** (it must be resumed — see *Stopped sessions and
  daemon restarts*); and a **restarted daemon binds a user's socket only when a session of that user starts or resumes**.
- **Telegram-started tool sessions get no host-session socket** (`TODO(stdio-relay)`), and no token or commit identity at
  all — see *Telegram-started sessions*.
- **Plain cursor-cli: no PR-tool token.** It has no toolcall listener at all, so there is nothing to answer.
- **Sandboxed cursor-cli resume is not implemented** (`resume_cursor_cli_session` ignores `sandbox`), so there
  is no relaunch to carry an identity.

## M6 — package documentation: `packages/tddy-accounts/docs/github-identity-resolution.md`

`packages/*/docs/` is changeset-driven (CLAUDE.md), so the file is **not created here**; its intended
content is below and is written into the package at wrap.

> # GitHub identity resolution
>
> ## What it answers
>
> Which GitHub account a project acts as — for the token *and* the commit identity — from one
> `acting_identity(assignments, provider, held)` call. `assignments` are the project row's `accounts`;
> `held` are the GitHub records the signed-in person's vault holds right now.
>
> ## Outcomes
>
> | Resolution | Result |
> |---|---|
> | `Assigned` | `ActingIdentity { account, token, git }` |
> | `NotAssigned` | `IdentityError::NotAssigned` — the project assigns no GitHub account |
> | `UnknownOnThisHost` | `IdentityError::UnknownOnThisHost` — assigned, but this host holds no such account |
> | `Ambiguous` | `IdentityError::Ambiguous` — two accounts at one provider; never picks |
> | `Assigned`, record carries no provider identifiers | `IdentityError::Unusable` — names the absent metadata key |
>
> Each refusal is a distinct message that names what the person would do about it.
>
> ## Identity derivation
>
> Name is the login (`META_SUBJECT`); email is `{META_SUBJECT_ID}+{login}@users.noreply.github.com`. The
> record's label is mutable and is **never** used. `session_git_environment` renders the four
> `GIT_AUTHOR_*` / `GIT_COMMITTER_*` pairs; the token is never among them.
>
> ## Where the resolution is taken
>
> At the session's edge, from `tddy-session-lifecycle`: `SessionAccountAccess::session_identity` returns the
> commit pairs and the handler that answers the agent's tools' `github-token`. The pairs are applied to the
> agent's process environment (co-located) or to the host-side relay environment (jailed). The token is
> **asked for, per call**, over the session's own toolcall socket — never an environment variable, never a
> file. Daemon-side operations (`RepointPlannedPr`) call `project_github_token` directly.
>
> ## No environment
>
> Nothing in `tddy-github` or `tddy-workflow-recipes` reads `GITHUB_TOKEN` / `GH_TOKEN`; a token reaches a
> REST call only as a parameter. Pinned structurally by
> `no_crate_still_resolves_a_github_token_from_the_process_environment` and behaviourally by the
> `GITHUB_TOKEN`-is-set tests at every seam.
>
> ## One consented exception
>
> A project that resolves no account does not stop a session starting: it starts with no `GIT_*`
> variables, under the checkout's own identity, and the reason is logged. It is not a token fallback.
>
> ## Unchanged
>
> WIP snapshots are signed by `tddy-daemon`, with the code's own reason.

## Verification (measured)

Scoped, not whole-workspace; CI is the authority on everything else. `./test -p <pkg>` (`--no-fail-fast`,
`--test-threads=1`), `.verify-result.txt` read, on 2026-10-08, after the tool-session identity and on-demand
recipe changes.

| Scope | passed | failed |
|---|---|---|
| `tddy-spawn` `tddy-session-lifecycle` `tddy-workflow-recipes` `tddy-pr-stack` `tddy-github` `tddy-telegram-control` `tddy-supervisor` | 1669 | 23 — all pre-existing, by name: 16 *"sandbox RPC bridge not installed"* (`sandboxed_session_*` ×5, `sandboxed_claude_cli_*` ×5, `sandboxed_cursor_cli_*` ×4, `resume/delete_sandbox_session_*` ×2); 6 `session_sync_livekit_acceptance` (`mirrors_*`, `removes_a_file_the_agent_deleted`, `restores_a_mirror_*`, `follows_the_session_head_*` — `tddy-remote-git-repo` not built, then *"Once instance has previously been poisoned"*); 1 `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent` (`/tmp` vs `/private/tmp`) |
| `tddy-accounts` `tddy-daemon-livekit` `tddy-daemon-auth` `tddy-toolcall` `tddy-tools` `tddy-daemon-rpc` | 1139 | 0 |

New tests, all green: `tddy-spawn` 3 in `spawn_worker.rs` (request carries / round-trips / old payload decodes empty)
and 5 in `tests/session_git_environment.rs`; `tddy-session-lifecycle` 6 in `tests/tool_session_git_identity.rs` and 2 in
`tests/supervisor_spawn_delegation.rs`; `tddy-workflow-recipes` 8 in `tests/orchestrate_pr_stack_github_on_demand.rs`;
`tddy-github` 4 in `pr_api.rs`. The failures above are untouched by this change and were not baselined against a clean
tree beyond the previous run's identical list.

Scoped `cargo check --all-targets` over `tddy-spawn`, `tddy-session-lifecycle`, `tddy-workflow-recipes`,
`tddy-pr-stack`, `tddy-github`, `tddy-supervisor`, `tddy-telegram-control`, `tddy-daemon`, `tddy-coder`,
`tddy-toolcall`, `tddy-tools` (every consumer of `SpawnOptions` / `SpawnRequest`): clean. `cargo clippy -D warnings
--all-targets` and `cargo fmt` over the six packages with source changes (`tddy-spawn`, `tddy-session-lifecycle`,
`tddy-workflow-recipes`, `tddy-pr-stack`, `tddy-github`, `tddy-telegram-control`): clean.

### Verification — tool-session token over the host-session socket (measured 2026-10-08)

Scoped, not whole-workspace. `./test -p tddy-toolcall -p tddy-host-service -p tddy-session-lifecycle -p tddy-coder
-p tddy-workflow -p tddy-workflow-recipes -p tddy-presenter -p tddy-spawn -p tddy-daemon -p tddy-tools -p tddy-accounts
-p tddy-daemon-livekit -p tddy-daemon-auth -p tddy-daemon-rpc -p tddy-github -p tddy-pr-stack`: **3204 passed, 24 failed**
(`./test` exits 0 regardless; the failures were read from the log). The 24: the 16 *sandbox RPC bridge not installed*
(`sandboxed_session_*` ×5, `sandboxed_claude_cli_*` ×5, `sandboxed_cursor_cli_*` ×4, `resume/delete_sandbox_session_*` ×2),
the 6 `session_sync_livekit_acceptance`, `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`
— all as before — and **one not in the earlier list**: `tddy-daemon-livekit` `session_room_livekit_acceptance::a_recorded_call_is_broadcast_into_the_room_stamped_with_the_tick_it_ran_in`,
which fails in the LiveKit testkit with *container startup timeout* (no Docker container here); that crate and its behaviour
are untouched by this change, but it was not re-run on a clean tree. `tddy-integration-tests --test workflow_runner_debug_log`
(call site of the new `run_workflow` parameter): 1 passed. Scoped `cargo clippy --all-targets -- -D warnings` over
`tddy-toolcall`, `tddy-host-service`, `tddy-session-lifecycle`, `tddy-coder`, `tddy-workflow`, `tddy-workflow-recipes`,
`tddy-presenter`, `tddy-integration-tests`, `tddy-spawn`, `tddy-daemon`, `tddy-tools`: clean; `cargo fmt --check`: clean for every
file this change touches (two untouched test files carry older format drift and were left as they were).

New tests: `tddy-host-service` 11 (`host_session_service`); `tddy-session-lifecycle` 13 lib (`host_session_socket_tests`: mode and
owner of the real socket and directory, path scheme, two sessions on one socket, wrong-user and unknown-session refusal, stale
socket replaced with modes kept, wider directory narrowed, live socket not stolen, non-socket never removed, removed file rebound,
unprivileged chown refused and nothing left, over-long path refused, unsafe user name refused) and 11 integration
(`tool_session_host_socket.rs`: every recipe gets the flag, owner-only socket, resume gets the same path, token over the real
socket, two sessions two accounts, no cross-session token, distinct verbatim refusals, `GITHUB_TOKEN` rescues nothing, delete
unregisters, **real coder listener → forwarding handler → real user socket → registry → account token**, and no socket → *no
credential handler*); `tddy-coder` 10 lib (`conversation_spawn_relay`, `tool_host_wiring`: request names the session, token and
refusal verbatim, unreachable socket named, reconnect after the host restarts and rebinds, a spawn never repeated, flag true
exactly when a handler is bound); `tddy-presenter` 2 (context seeded with the flag); `tddy-spawn` 1 (`GITHUB_TOKEN`/`GH_TOKEN` not
inherited — it **failed first**, with both lines present — and the existing environment-record test was made to wait for the
child's record, the race that made it fail intermittently).

Mutation-checked, each restored afterwards: skip the OS-user check → `…another_os_user…` fails (host-service and lifecycle unit;
**not** the integration file, which has one OS user); ignore the session id when looking up → 3 host-service, 1 socket unit and 4
integration tests fail; socket mode `0o666` → 2 unit and the integration owner-only test fail; directory mode `0o755` → 3 unit and
the integration owner-only test fail; steal a live socket → `a_live_socket_is_not_stolen` fails (unit only); skip `unregister` on
delete → `a_deleted_session_is_no_longer_answered` fails (integration only); start passes no socket → 10 of 11 integration tests
fail. Not every test of this change was written before its code: the host-service module and the wiring were implemented first and
their tests written against them (the socket module and the inheritance fix were red first).

### Verification — closing the deferrals (measured 2026-10-08)

Scoped, not whole-workspace; CI is the authority on everything else. `./test -p tddy-host-service -p tddy-session-lifecycle
-p tddy-coder -p tddy-toolcall -p tddy-github -p tddy-workflow-recipes -p tddy-telegram-control -p tddy-spawn -p tddy-supervisor
-p tddy-daemon -p tddy-tools -p tddy-daemon-rpc -p tddy-accounts -p tddy-daemon-livekit -p tddy-daemon-auth -p tddy-pr-stack
-p tddy-presenter -p tddy-workflow`, `.verify-result.txt` read: **3569 passed, 25 failed** (`./test` exits 0 regardless). The 25:
the known 16 *sandbox RPC bridge not installed*; 7 in `session_sync_livekit_acceptance`/`session_agent_remote_acceptance`
(6 `session_sync_livekit_acceptance` — *tddy-remote-git-repo* not built, then a poisoned `Once` — plus
`session_agent_remote_acceptance::restores_a_clone_that_diverged_and_says_so`, which **passes when run alone** (14.7 s) and whose
own doc comment already names a contended host as the likely cause; it is not on the earlier list and is flaky under the full
run, not caused by this change); `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`
(`/tmp` vs `/private/tmp`); and `session_room_acceptance::a_session_started_while_livekit_was_down_is_drivable_over_livekit_once_it_is_back`
(*container startup timeout* — no Docker; the same cause as the previously-listed `session_room_livekit_acceptance` failure, a
different test in a different crate). `cargo check --all-targets` over the six packages with source changes plus `tddy-daemon-rpc`,
`tddy-pr-stack`, `tddy-spawn`, `tddy-toolcall`, `tddy-tools`, `tddy-daemon` (every consumer of a changed signature): clean.
`cargo clippy --all-targets -D warnings` over `tddy-host-service`, `tddy-session-lifecycle`, `tddy-coder`, `tddy-github`,
`tddy-workflow-recipes`, `tddy-telegram-control`: clean; `cargo fmt` applied to them (two untouched test files carry older format
drift and were left as they were).

Red first, by kind: *failed on behaviour* — the two refused-at-start tests (`Ok("ghp_…")` where a refusal was required),
`a_telegram_started_session_on_a_project_that_acts_as_an_account_says_the_account_is_unavailable` (no notice). *Failed to compile*
(the API they name did not exist) — the host-service probe and stop-report tests, the stop/restart integration tests
(`with_host_session_stop_watch_interval`) and the four REST tests (`with_api_base`). *Written with their code, never seen red* —
the `…keeps_refusing`-adjacent guards that pass on both implementations, `one_os_users_socket_cannot_fetch_another_os_users_session_token`
(it exercises code that already existed; its value is the mutation below), the unprivileged-chown message assertion, and the coder's
*resume* hint on an unreachable socket.

Mutation-checked, each restored afterwards: **skip the OS-user check → `one_os_users_socket_cannot_fetch_another_os_users_session_token`
fails in the integration file** (previously unit tests only; the two sockets there are the daemon's own service over real sockets with
the owners named directly — a daemon cannot bind a socket for a second real OS user without privilege, so a two-user test through
`DaemonSessionHost` is not possible unprivileged); no `watch_until_stopped` on spawn → `a_tool_session_whose_process_has_stopped_is_no_longer_answered`
fails; existence probe always false → that test and `after_a_daemon_restart_a_running_session_is_told_to_resume_and_then_is_answered` fail;
a wrong token in the REST `Authorization` header → `a_rest_call_carries_the_token_the_session_host_returned` and
`a_github_token_in_the_environment_authenticates_no_call` fail. Not mutation-checked: `unregister_stopped`'s pid condition (covered by its
unit test and `a_resumed_session_stays_answered_when_its_earlier_process_stops`, not by a deliberate break).

## Green wave

**Wave 5 of 5** — alone.

```
waves   1:{n1}   2:{n2, n3}   3:{n4}   4:{n5, n6, n7, n8}   5:{n9}
line    n1 · n2, n3 · n4 · n5, n6, n7, n8 · n9
```

Nothing depends on it, and it depends on all of wave 4 plus an out-of-stack PR. It is last in the
line and last in time, which is what the developer asked for.

## Prerequisites

### ⚠ UNBLOCKED BY THE BASE, NOT YET RESOLVED BY IT — the GitHub REST client is mid-move — [`squatting-github-rest-client`](../../../packages/tddy-workflow-recipes/docs/code-issues/squatting-github-rest-client.md)

**Status in the record: `Open — claimed by #492, in flight`.** It is another stack's to fix
(`#carve` 6/10), and this node neither claims it nor touches the files — **it must not delete the
record**, which is #492's to delete when it wraps.

This was a ⛔ BLOCKING entry when the stack was cut. Re-basing the `#keyring` root from `master`
onto `feature/carve/git-plumbing` (#492) on 2026-09-19 removed the *collision* — this node will never
edit a crate the files are leaving. ⚠ **It did not move the files.** #492 is an ancestor in its own
red state, so on 2026-09-20 this tree still has the client in `tddy-workflow-recipes` and
`tddy-github` has no PR surface. The entry therefore stays **⚠ DURING**: it constrains this node's
green phase, which must thread the token parameter through wherever #492 has put the file by then.

The record's own guidance is what made it blocking:

> Coordinate if you are **adding a REST call** — it belongs in `tddy-github` after #492, and adding
> it here means #492 moves it too.

This node does something stronger than add a call: it changes how **all 2,124 lines** of that client
get their token. Doing it *before* the move would mean editing a crate those files are leaving, and
then having #492 rewrite the result. Basing the stack on #492 removes that collision entirely rather
than scheduling around it.

**The developer decided this before the stack was cut**, which is why it is recorded rather than
escalated:

> plan the changes to git/github as late as possible with a dependency of this PR being merged
> https://github.com/uppin/tddy-coder/pull/492

⚠ **The cost moved from this node to the whole stack.** Under the original plan nodes 1–8 were
independent of #492 and only this node waited. Now every `#keyring` node lands behind `#carve` 3/10
[#498](https://github.com/uppin/tddy-coder/pull/498), 4/10
[#491](https://github.com/uppin/tddy-coder/pull/491) and 5/10
[#492](https://github.com/uppin/tddy-coder/pull/492). That trade was taken deliberately — it is the
cheapest possible moment for the rebase, since every `#keyring` node is still docs-only — and it is
recorded in node 1's changeset.

### ⚠ DURING — `runtime::build` complexity — [`complexity-runtime-build`](../../../packages/tddy-daemon/docs/code-issues/complexity-runtime-build.md)

Wiring the resolver into the session path passes through it. Recorded, not claimed.

### ℹ ANSWERED — what replaces `FileGitHubTokenStore`

Its doc comment states the constraint that produced it:

> The daemon runs from a systemd unit with no `GITHUB_TOKEN` in its environment, so the only
> credential it can use for an operator's PR reads is the one that operator granted at login.

Correct, and narrower than what the stack now needs: the file is `login -> access_token`, one token
per login name, with no notion of a project. 3/9 replaced the storage; this node removes the last
readers. Answered, so the entry is recorded and **not** deleted.

### Unanalyzed packages

`tddy-daemon-livekit` and `tddy-tools`' code-issue records were not scanned for this node beyond the
two above. **"Not measured" is not "clean"**; this node claims nothing about them.

## Scope

- [x] **PRD**: [PRD-2026-09-19-keyring-github-identity.md](../../ft/daemon/1-WIP/PRD-2026-09-19-keyring-github-identity.md)
- [x] **Changeset**: this document
- [x] ⚠ **Unblocked by the base, not yet supplied by it**: #492 is an ancestor, so the merge-order
      collision is gone — but it is still red, so the post-move module paths are **not** in this tree
      and the REST half of the contract is deferred to this node's green phase
- [x] **Draft PR contract**: surface + failing tests (wave 2, commit 2) — ⚠ **partly**: the REST
      half is deferred, see **As published** under `## Draft PR contract`
- [x] **Resolution**: one call, token and identity from one answer — the account is chosen **once** at start/resume (`SessionAccountAccess::session_identity`); the commit pairs derive from that `ActingIdentity` and the token handler is pinned to its outcome (the account's id, or the refusal), so the token is fetched per call but only ever for that account — see *One resolution, pinned at the session's start*
- [x] **Outcomes**: a distinct failure per `AccountResolution` variant — `project_resolved_identity_acceptance.rs`, `acting_identity_unit.rs`, and the lifecycle/daemon-rpc refusal tests
- [x] **Deletion**: the environment resolution path; `FileGitHubTokenStore`'s readers — `login_time_token_store_is_retired.rs` (4 structural tests)
- [x] **Testing**: unit + acceptance, scoped — see *Verification (measured)* and *Verification — closing the deferrals*
- [x] **Package Documentation** — *recorded, not written into the package*: the content of `packages/tddy-accounts/docs/github-identity-resolution.md` is under *M6* above and the file is created at wrap, because CLAUDE.md forbids editing `packages/*/docs/` directly (changeset workflow). Ticked as a recorded plan, not as a file that exists
- [ ] **Code Quality**: scoped clippy ✅ (`-D warnings`, `--all-targets`, the six packages with source changes — see *Verification (measured)*); ⚠ CI green not yet read

## Technical Changes

### State A (Current)

- The REST client resolves `GITHUB_TOKEN` then `GH_TOKEN` from the **process** environment — one
  GitHub identity per daemon, for every project it serves.
- `FileGitHubTokenStore` keeps `login -> access_token`, unaware of projects.
- The agent's commits carry whatever identity the checkout is configured with.

### State B (Target)

- A session's git environment carries the **assigned account's** token, name and email, from one
  resolution.
- No environment fallback exists. An unassigned project cannot perform a GitHub operation.
- `FileGitHubTokenStore` has no readers.
- WIP snapshots are still authored by `tddy-daemon`.

### Delta (What's Changing)

#### tddy-github (after #492)
- **API**: REST entry points take a token. `github_token_from_env` and its callers are **deleted** —
  not deprecated, because a deprecated environment read is still an environment read.

#### tddy-daemon-livekit
- **Implementation**: the session's environment is built from one `AccountResolution`. The token and
  the four `GIT_*` variables come from the same record, in one place, so a mismatch is not
  constructible.
- **Unchanged**: `WIP_COMMIT_IDENTITY_NAME` / `_EMAIL` and the `commit-tree` identity block.

#### tddy-accounts
- **Implementation**: one failure message per `AccountResolution` variant, each naming what the person
  would do about it.

#### tddy-daemon-auth
- **Implementation**: `FileGitHubTokenStore`'s remaining readers removed.

### Decision recorded with its alternative

**The environment variable is the seam.** `git` and `gh` are ordinary subprocesses; setting
`GITHUB_TOKEN` and the four `GIT_*` variables makes every invocation correct without either tool
learning anything about this stack.

⚠ **Worth challenging**: the variable is visible to every process in the session, including the agent,
which can print it. A credential helper handing a token to `git` per invocation never exposes it —
but it is considerably larger, and it does not cover `gh` or the agent's own API calls, which *need*
the token. Recorded so the reviewer can weigh it; a helper can be added later without changing the
resolution this node builds.

## Implementation Milestones

✅ **M1 is not gated on #492 merging** — #492 is this stack's base, so its move is already in the tree.

- [x] **M1** — REST entry points take a token; environment resolution deleted
- [x] **M2** — one resolution at the session edge; token + identity from it — every session path this daemon owns (see *Closing the remaining seams*), and the commit identity now reaches tool sessions (`tddy-coder`) over the spawn wire; their agent's *token* too, over the per-OS-user host-session socket
- [x] **M3** — a distinct failure per outcome
- [x] **M4** — retire `FileGitHubTokenStore`'s readers — already true on the base (`#keyring` 3/9–8/9); the three structural tests were green before this node's green phase touched anything
- [x] **M5** — acceptance: two projects, two accounts, one daemon — `project_resolved_identity_acceptance.rs`
- [x] **M6** — `packages/tddy-accounts/docs/github-identity-resolution.md` — content written under *M6* above; file created at wrap (CLAUDE.md: `packages/*/docs/` is changeset-driven)

## Testing Plan

### Testing Strategy

**Two tests carry this node, and both are negative.**

The first: with a project that has **no** account assigned and `GITHUB_TOKEN` set in the daemon's
environment, a GitHub operation **fails**. Every positive test in this plan passes with a fallback in
place; this is the only one that does not, and what it prevents is a push succeeding as the wrong
person.

The second: the token and the commit identity come from **one** resolution. Resolving twice works
perfectly until an assignment changes between the two calls, and then produces a commit authored by
one account and pushed by another — which reads, to everyone downstream, as the first account's work.

### Unit tests

- Each `AccountResolution` variant maps to its own failure, with its own message.
- The session environment is constructed from a single record — asserted by construction, not by
  comparing two lookups.
- No code path reads `GITHUB_TOKEN` or `GH_TOKEN` from the process environment — asserted
  structurally over `tddy-github` and `tddy-workflow-recipes`, since a deleted function is
  what makes the claim true rather than an unexercised branch.

### Acceptance tests

- A commit in a session carries the assigned account's name and email.
- A GitHub API call uses the assigned account's token.
- Two projects, two accounts, one daemon: different GitHub users.
- `NotAssigned` + `GITHUB_TOKEN` exported → **fails**.
- `UnknownOnThisHost` fails with its own reason.
- WIP snapshot commits are still authored by `tddy-daemon`.

### Verification scope

`./test -p tddy-accounts -p tddy-github -p tddy-daemon-livekit -p tddy-daemon-auth` and scoped
clippy. LiveKit-backed tests reuse the testkit container. Whole-workspace green comes from CI via
`scripts/ci-status.sh`.

## Acceptance Criteria

- [x] A commit carries the assigned account's name and email — `a_commit_made_in_a_session_is_authored_by_the_account_the_project_assigns`, `a_commit_made_in_a_session_is_committed_by_the_same_account_that_authored_it` (`session_git_identity_acceptance.rs`), and `an_assigned_project_gives_the_session_its_pairs_and_a_handler_for_the_same_account` / `tool_session_git_identity.rs` for the pairs reaching the agent's env; ⚠ no test runs a real `git commit` under them
- [x] A tool session's agent obtains the assigned account's token — `a_tools_token_request_reaches_the_assigned_account_through_the_coder_and_the_host_socket`
- [x] Tool-session sockets are owner-only (0600 in 0700) and per OS user — `the_socket_a_session_is_given_is_owner_only_in_an_owner_only_directory`, `host_session_socket_tests`; ⚠ **only where the daemon can `chown`** — under `tddy-supervisor` it cannot and there is no socket (see *Still open*)
- [x] A GitHub API call uses the assigned account's token — `a_rest_call_carries_the_token_the_session_host_returned` (the request's `Authorization: Bearer` is the token `request_github_token_from_session()` returned from a fake host over a real socket), with `a_refusing_host_means_no_request_reaches_the_api`; that the host answers the *assigned* account's token is `a_tools_token_request_reaches_the_assigned_account_through_the_coder_and_the_host_socket`
- [x] Two projects assigned different accounts act as different GitHub users — `two_projects_on_one_daemon_act_as_different_github_users`
- [x] Token and identity always come from **one** resolution — `the_commit_pairs_and_the_token_derive_from_one_record`, `a_start_refused_because_the_vault_was_locked_keeps_refusing_after_it_is_unlocked`, `a_start_refused_for_an_unheld_account_keeps_refusing_once_the_account_arrives`, `the_token_stays_the_started_accounts_when_a_second_account_appears`, `the_started_accounts_token_is_never_replaced_by_another_accounts_when_it_is_removed`; `the_token_and_the_git_identity_name_the_same_account` pins the one function
- [x] `NotAssigned` fails; **no environment fallback** — `a_project_that_assigns_no_account_is_refused_although_the_environment_holds_a_token`, `a_start_refused_for_no_assignment_is_not_rescued_by_the_environment_or_a_later_assignment`, `a_github_token_in_the_daemons_environment_rescues_an_unassigned_project_from_nothing`, `a_github_token_in_the_environment_authenticates_no_call`
- [x] `UnknownOnThisHost` fails with its own reason — `an_account_this_host_has_never_received_is_refused_in_its_own_words`, `the_credential_handler_tells_an_unheld_assignment_apart_from_no_assignment`, `an_unassigned_and_an_unknown_account_are_refused_distinctly_and_verbatim`
- [x] `Ambiguous` fails rather than picking — `two_accounts_assigned_at_one_provider_are_refused_rather_than_one_being_picked` (`tddy-accounts`), `two_accounts_assigned_at_one_provider_is_ambiguous_and_names_that_provider`; ⚠ no *lifecycle*-level test drives an ambiguous assignment through the handler (it would be pinned as `Refused` by construction, the same path as `NotAssigned`)
- [x] `GITHUB_TOKEN` in the daemon's environment is never used for a session — `a_github_token_in_the_daemons_environment_rescues_nothing_and_replaces_nothing`, `a_github_token_in_the_daemons_environment_rescues_an_unassigned_project_from_nothing`, `a_github_token_in_the_environment_authenticates_no_call`, and the spawn-inheritance test in `tddy-spawn`
- [x] `github_token_from_env` is deleted — no crate, daemon-side or agent-side, reads a
      GitHub token from the process environment — `no_crate_still_resolves_a_github_token_from_the_process_environment`
- [x] WIP snapshot commits are still authored by `tddy-daemon` — `a_work_in_progress_snapshot_is_still_signed_by_the_daemon_itself`
- [x] `FileGitHubTokenStore` has no readers left — `login_time_token_store_is_retired.rs` (`the_session_path_no_longer_reaches_for_the_operator_s_login_time_github_token`, `the_authentication_service_no_longer_retains_a_github_token_beside_the_vault`, `nothing_in_the_tree_still_declares_a_login_keyed_github_token_store`)
- [x] A stopped session stops being answered, a restarted daemon's running session is told to resume — `a_tool_session_whose_process_has_stopped_is_no_longer_answered`, `after_a_daemon_restart_a_running_session_is_told_to_resume_and_then_is_answered`
- [ ] A session of an OS user the daemon cannot `chown` to has the host-session socket — **not met**: blocked on a supervisor design decision, see *Still open*

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [x] Publish the draft-PR contract — wave 2 (⚠ REST half deferred to green)
- [x] ⚠ #492 is the stack's base — no separate wait, but it is still red: the moved module paths
      arrive only when it greens
- [x] M1–M6
- [x] `packages/tddy-accounts/docs/github-identity-resolution.md` — content in this changeset (*M6*); file at wrap
- [ ] `/wrap-context-docs` — this node claims **no** `docs/dev/todo/` entry and **no** code-issue
      record. `squatting-github-rest-client.md` belongs to **#492** and must not be deleted here

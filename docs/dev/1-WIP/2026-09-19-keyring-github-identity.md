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
  token; the prompt awareness gate reads a context flag instead of a hardcoded `false`

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
  a context flag; **nothing sets it in production yet**, see *Still open*.

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

Closed — see *Closing the remaining seams*. Still open: tool-session (`tddy-coder`) starts, see *Still open*.

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

- The assignments are a **snapshot taken at session start/resume** (the commit identity's snapshot, so
  a commit and a push in one session cannot come from two accounts because the project was reassigned
  between them). A reassignment takes effect on the next start or resume. The vault is read **per
  call**, so a vault locked since, or an owner whose session token expired, refuses at the call.
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
- `tddy-tools` is verified against a real socket and a real `ToolcallRpcService`, but the *token's use
  at GitHub* is not: `RealGithubPrApi` shells out to `api.github.com`, so the tests prove the token is
  asked for, the refusal surfaces, and a client is built from the answer — not that the REST call
  carries it;
- tool-session (`tddy-coder`) sessions and the engine-driven prompt flag — see *Still open*.

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

**Agent-process workflow actions** (`AssessTask`, `MergeTask`, `RepointTask`) ask the host with
`tddy_core::toolcall::request_github_token_from_session()` — the same `github-token` round trip `tddy-tools`
makes, reading `TDDY_SOCKET` only to learn *whom to ask*. The socket client moved to `tddy-toolcall`
(`request_github_token`); both `tddy-tools` and these crates reach it through `tddy-core`'s facade. It could
not sit in `tddy-tools`, which depends on `tddy-workflow-recipes`. ⚠ These tasks belong to
`OrchestratePrStackRecipe`, which is **retained but inert in production** (`recipe_resolve` maps every CLI
name to `PrStackRecipe`); they have no live driver, and the request is made eagerly, so an `assess` over a
stack with nothing for GitHub to answer now needs a host too. The live callers of the same helpers
(`assemble_views`, `execute_stack_merge`, `execute_stack_repoint`) are the `pr_*` tools, which already pass a
token.

**Prompt awareness.** `merged_red_system_prompt` and the merge-pr system prompts advertise the PR tools only
when the workflow context carries `github_pr_tools_available = true`
(`tddy_workflow_recipes::github_pr_tools`). Neither hardcodes `false` any more, and neither probes the
environment.

### Still open

- **Tool sessions (`tddy-coder` via the supervisor/worker wire)** start under the checkout's commit identity
  and their agent's PR tools are refused ("its listener has no credential handler"). `tool_session_spawn.rs`
  carries a `TODO(keyring 9/9)`. A fix is a **wire change**: `SpawnRequest` has no env field and the child has
  no channel back to the daemon's `SessionGithubCredential`. Not done — a decision, not an edit.
- **The prompt flag is set by nobody.** The hooks that read `github_pr_tools_available` run in a `tddy-coder`
  process, whose listener has no credential handler for the same reason; until the wire above exists the
  prompts truthfully stay silent. `TODO(keyring 9/9)` in `tddy-workflow-recipes/src/github_pr_tools.rs`. A
  daemon-managed claude-cli session never runs these hooks.
- **Plain cursor-cli: no PR-tool token.** It has no toolcall listener at all, so there is nothing to answer.
- **Sandboxed cursor-cli resume is not implemented** (`resume_cursor_cli_session` ignores `sandbox`), so there
  is no relaunch to carry an identity.
- The commit identity and the token remain **two resolutions over one assignment snapshot** (see Scope).

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
`--test-threads=1`), `.verify-result.txt` read, on 2026-10-07/08.

| Scope | passed | failed |
|---|---|---|
| `tddy-session-lifecycle` | 605 | 22 — all pre-existing: 16 *"sandbox RPC bridge not installed"* (`sandboxed_session_*`, `sandboxed_claude_cli_*`, `sandboxed_cursor_cli_*`, `resume/delete_sandbox_session_*`) and 6 `session_sync_livekit` (`tddy-remote-git-repo` not built, then *"Once instance has previously been poisoned"*) |
| `tddy-accounts` `tddy-daemon-livekit` `tddy-daemon-auth` `tddy-toolcall` `tddy-tools` | 888 | 0 |
| `tddy-workflow-recipes` `tddy-pr-stack` `tddy-github` `tddy-daemon-rpc` | 898 | 1 — `pr_stack_artifact_paths_acceptance::a_plan_left_at_the_legacy_session_root_is_still_advertised_to_the_agent`: the expected path is `/tmp/nix-shell…/pr-stack-plan.md` and the actual one `/private/tmp/nix-shell…/pr-stack-plan.md`, i.e. macOS's `/tmp` symlink; no file this change touches is in that path. Not baselined against a clean tree |

Scoped `cargo clippy -D warnings --all-targets` and `cargo fmt` over the seven touched packages
(`tddy-session-lifecycle`, `tddy-daemon-rpc`, `tddy-github`, `tddy-workflow-recipes`, `tddy-toolcall`,
`tddy-tools`, `tddy-pr-stack`): clean.

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
- [ ] **Resolution**: one call, token and identity from one answer — ⚠ **open by design**: both come from the same `acting_identity` function over one assignment snapshot, but the commit pairs (at start) and the token (per call, so a locked vault refuses) are two *calls*
- [x] **Outcomes**: a distinct failure per `AccountResolution` variant — `project_resolved_identity_acceptance.rs`, `acting_identity_unit.rs`, and the lifecycle/daemon-rpc refusal tests
- [x] **Deletion**: the environment resolution path; `FileGitHubTokenStore`'s readers — `login_time_token_store_is_retired.rs` (4 structural tests)
- [x] **Testing**: unit + acceptance, scoped — see *Verification (measured)*
- [x] **Package Documentation**: `packages/tddy-accounts/docs/github-identity-resolution.md` — content written under *M6* above; the file itself is created at wrap, since `packages/*/docs/` is changeset-driven
- [ ] **Code Quality**: scoped clippy ✅ (`-D warnings`, `--all-targets`, the seven touched packages); ⚠ CI green not yet read

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
- [x] **M2** — one resolution at the session edge; token + identity from it — every session path this daemon owns (see *Closing the remaining seams*); ⚠ tool sessions (`tddy-coder`) remain, behind a wire change — see *Still open*
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

- [x] A commit carries the assigned account's name and email — `session_git_identity_acceptance.rs` (author and committer pairs), and the lifecycle tests that the pairs reach the agent's env; ⚠ no test runs a real `git commit` under them
- [ ] A GitHub API call uses the assigned account's token — ⚠ open: proven as far as the token being asked for and a client built from it; no test asserts the REST call carries it (`RealGithubPrApi` shells out to `api.github.com`)
- [x] Two projects assigned different accounts act as different GitHub users — `two_projects_on_one_daemon_act_as_different_github_users`
- [ ] Token and identity always come from **one** resolution — ⚠ open, same reason as *Resolution* in Scope; `the_token_and_the_git_identity_name_the_same_account` pins the one function
- [x] `NotAssigned` fails; **no environment fallback** — `a_project_that_assigns_no_account_is_refused_although_the_environment_holds_a_token`, and the `GITHUB_TOKEN`-set tests at the session edge, the tools, and the toolcall client
- [x] `UnknownOnThisHost` fails with its own reason
- [x] `Ambiguous` fails rather than picking
- [x] `GITHUB_TOKEN` in the daemon's environment is never used for a session
- [x] `github_token_from_env` is deleted — no crate, daemon-side or agent-side, reads a
      GitHub token from the process environment — `no_crate_still_resolves_a_github_token_from_the_process_environment`
- [x] WIP snapshot commits are still authored by `tddy-daemon`
- [x] `FileGitHubTokenStore` has no readers left

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

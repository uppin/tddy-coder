# 2026-09-24 — `restructure` drops comments from the code it moves, and writes extract-method signatures clippy rejects

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), plans
`09b` (plan `09` without op 6), `02-cli-session-manager-dir`, `10b`, `09c` and `11`–`16`,
change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)

Both of these showed up only **after** the compile gate was satisfied: `cargo check --all-targets`
was clean, and `cargo test -p tddy-session-lifecycle` matched the baseline. Putting the comments
back and reshaping the signatures are both edits beyond a build correction, so plan `09b` was first
held for the developer.

**Landed on #524 by hand** (developer, 2026-09-24: "Restore comments and sigs, file TODO for the
engine fix"). The 14 comment lines went back unreworded, each beside its statement in the function
it moved into. The signatures were reshaped with no `#[allow]` (see "After the hand fix" under Q).
The engine defects below are still open. This file is what tracks them.

## P — `extract_method` drops the comments inside the range it extracts

Plan `09b`, 7 × `extract_method` in
`connection_service/svc_start_sandboxed_claude_cli_session.rs`. **14 comment lines** in the extracted
ranges are gone from the file. The comment-line multiset shrank from 94 to 80, and no comment
moved anywhere else.

```rust
// before: lines 105–108, inside `warm_up_jail_agents`'s range
        let mut started_agents = self.seeded_roster_records(specialized_agents).await?;
        // The defs behind those records, which the jail env can only carry for agents this host
        // holds — the records above are what carries the rest.
        let specialized_defs = self

// after: the new fn, and the comment is nowhere in the file
    async fn warm_up_jail_agents(&self, specialized_agents: &[String]) -> Result<…, Status> {
        let mut started_agents = self.seeded_roster_records(specialized_agents).await?;
        let specialized_defs = self
```

The others lost were:
- the readiness-gate rationale ("wake every specialized agent's endpoint … No fallback — the jail is
  never spawned if warm-up fails");
- the Seatbelt canonical-path explanation ("without this the tool-IPC socket bind fails with
  'Operation not permitted'");
- the `scratch_home` note.

These are exactly the comments that say *why*.

`extract_module` lost one too. Plan `02` dropped a section banner that sat at its `livekit_bridge`
seam:

```rust
// before: cli_session_manager.rs
// ---------------------------------------------------------------------------
// LiveKit bridge: expose a PtyHandle as a LiveKit RPC server
// ---------------------------------------------------------------------------

// after: in neither cli_session_manager.rs nor cli_session_manager/livekit_bridge.rs
```

Plans `03`, `10a`, `04`, `08`, `06` and `07` lost none, measured the same way against their parent
commits.

**Suspected cause, unconfirmed:** the assist rebuilds the body from syntax nodes and drops the
trivia between statements.

**Check it cheaply:** compare the multiset of `//` lines under the crate before and after the
run, as the statement-multiset check in `restructure verify` does for statements.

## Q — extracted signatures take `&PathBuf`/`&String`/`&Vec<_>` and any number of parameters

The same plan leaves `cargo clippy -p tddy-session-lifecycle --all-targets -- -D warnings` with 18
findings, every one in a signature or body the engine wrote:

```rust
// after plan 09b, formatted
    fn managed_jail_env(
        &self,
        os_user: &str,
        session_id: &str,
        sessions_base: PathBuf,
        managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        project_id: &str,
        session_dir: &PathBuf,        // clippy::ptr_arg
        worktree_path: &PathBuf,      // clippy::ptr_arg
        context_dir: &PathBuf,        // clippy::ptr_arg
        tddy_tools_path: &String,     // clippy::ptr_arg
    ) -> Result<(Option<…ManagedWorkflow>, Option<PathBuf>, Vec<(String, String)>), Status>
    //         clippy::too_many_arguments (10/7), clippy::type_complexity

fn prepare_jail_dirs(session_dir: &PathBuf) -> Result<(PathBuf, PathBuf, PathBuf, PathBuf, PathBuf), Status>
```

```text
error: writing `&PathBuf` instead of `&Path` involves a new object where a slice will do   (×12)
error: writing `&String` instead of `&str` involves a new object where a slice will do    (×1)
error: writing `&Vec` instead of `&[_]` involves a new object where a slice will do       (×1)
error: this function has too many arguments (10/7)   managed_jail_env
error: this function has too many arguments (16/7)   launch_jail
error: this function has too many arguments (8/7)    write_jail_session_metadata
error: very complex type used. Consider factoring parts into `type` definitions
error: variable does not need to be mutable          warm_up_jail_agents: `let mut started_agents`
```

- The `ptr_arg` findings come from the assist borrowing each local by its own type. `&Path` would
  be what a person writes, and the call sites would not change.
- The `mut` is left over. The original function mutates `started_agents` after the extracted
  range, so the binding is `mut` in the caller, but the assist copied the `mut` into the callee.
- Plan `10a`'s `spawn_tddy_coder` (22 parameters) is the same shape. It carries
  `#[allow(clippy::too_many_arguments)]`, the crate's existing pattern, until DRY #2 folds it into a
  `ToolSpawnPlan`. The crate has **no** precedent for allowing `ptr_arg` or `type_complexity`.

### After the hand fix (#524)

This is what a person writes, and the engine should come as close to it as it can:

```rust
/// The session a sandboxed start is building a jail for: who it is and where it lives on the host.
struct JailSession<'a> {
    session_id: &'a str,
    project_id: &'a str,
    session_dir: &'a Path,
    worktree_path: &'a Path,
}

/// A managed-workflow session's controller, its orchestration prompt file and its host-side env.
type ManagedJailEnv = (
    Option<crate::session_toolcall::ManagedWorkflow>,
    Option<PathBuf>,
    Vec<(String, String)>,
);

    fn managed_jail_env(
        &self,
        jail: &JailSession<'_>,
        os_user: &str,
        sessions_base: &Path,
        managed_recipe: &Option<Arc<dyn tddy_core::workflow::recipe::WorkflowRecipe + 'static>>,
        context_dir: &Path,
        tddy_tools_path: &str,
    ) -> Result<ManagedJailEnv, Status>

fn prepare_jail_dirs(session_dir: &Path) -> Result<JailDirs, Status>   // a named struct, not a 5-tuple
async fn launch_jail(&self, jail: &JailSession<'_>, jail_dirs: &JailDirs, launch: JailLaunch)
fn write_jail_session_metadata(jail: &JailSession<'_>, model: &str, managed_recipe: …, started_agents: …, pid: u32)
```

The 14 `ptr_arg` fixes and the `mut` are mechanical, and the engine could make them. The
`JailSession`, `JailDirs` and `JailLaunch` structs and the `ManagedJailEnv` alias are judgement calls
about names and grouping. That is a reason for the engine to report them rather than invent them.

### Plan `05` (#524): `ptr_arg` again, and a unit value wrapped in `Ok`

Plan `05`'s four extract-methods on `spawn_split_agent` compile, and leave four clippy findings:
three `ptr_arg` of the same shape as above, and one new one. The range the assist extracted is an
`if` statement, an expression of type `()`, so it became the function's tail, wrapped in `Ok`:

```rust
// after plan 05, formatted
    async fn join_split_livekit_room(
        &self,
        …
        session_dir: &std::path::PathBuf,          // clippy::ptr_arg
    ) -> Result<(), Status> {
        Ok(if livekit.is_some() {                  // clippy::unit_arg
            …
        })
    }

// by hand (#524)
        session_dir: &std::path::Path,
    ) -> Result<(), Status> {
        if livekit.is_some() {
            …
                session_dir.to_path_buf(),         // was `session_dir.clone()`, for `RemoteCheckout::new`
            …
        }
        Ok(())
    }
```

`split_agent_context_and_args` took `session_dir` and `tddy_tools_path` as `&std::path::PathBuf`,
fixed the same way. The unit tail is mechanical too: a unit-typed tail belongs before `Ok(())`, not
inside it.

### Plan `10b` (#524): the same P and Q, three build breaks, a refusal and a hang

Plan `10b` re-cuts plan `10`'s refused extract-methods on `start_session_core` into 15 ranges that
hold no `return`. Two of them are **expressions after a `return`**
(`return self.start_sandboxed_claude_cli_session(…).await;` loses everything after `return `), so
the long argument lists move and the early exit stays. It applied 15 of 15 and the compile gate
failed. The gate printed only the first error class (K, 5 × `WorkflowRecipe`); the rest appeared
after the K fix.

**P:** 22 comment lines were lost. The biggest was the 6-line "Written before this call answers"
rationale for writing the roster before the workspace start returns. All went back unreworded.

**Three build breaks the gate did not reach**, each fixed by hand:

```rust
// 1. a deref written before a macro statement (E0614: type `()` cannot be dereferenced)
        ) {
            *log::warn!(
                "StartSession: could not remove session {session_id} after its \
                 jail could not be provisioned: {}",
// by hand: `log::warn!(`

// 2. a value passed by move while a reference into it is passed too (E0505)
        let repo_path = Path::new(&project.main_repo_path);
        …
            .spawn_tool_session(req, progress, os_user, agent_def, livekit, project, repo_path)
// by hand: `&project`, and `repo_path` derived inside from the project (see Q below)

// 3. an expression range after `return` borrows what the callee consumes (E0308 ×2)
    async fn start_sandboxed_claude_cli_from_request(&self, …,
        sessions_base: &std::path::PathBuf,                 // the callee takes `PathBuf`
        managed_recipe: &Option<Arc<dyn WorkflowRecipe + 'static>>,   // the callee takes it by value
// by hand: by value, and `&String`/`&Option<String>` → `&str`/`Option<&str>`
```

**Q:** `unit_arg` again (`Ok(if !project_id.is_empty() { … })`), `ptr_arg` ×3,
`too_many_arguments` ×4 (8/7, 8/7, 9/7, 8/7), and an unused `mut` in the **caller**. The callee's
`&mut started_agents` is now the only mutation, so the caller's binding no longer needs `mut`, which
is the reverse of the `09b` case. The hand fix follows `09b`, with no `#[allow]`:
- a file-local `CliStart` (sessions base, session id, initial prompt) is what `cli_start_prelude`
  returns, in place of a 3-tuple, and the four `*_from_request` functions take it;
- `spawn_tool_session` derives `repo_path` from the project it is handed, rather than taking both.

**Refused (S)** at `check --deep`. The range was an expression, the `if … else` value of a `let`:

```jsonl
{"op":"extract_method","anchor":{…"start":{"line":558,"col":17},"end":{"line":567,"col":18}},"name":"managed_recipe_for"}
```

```text
3: rust-analyzer's answer was unusable: rust-analyzer did not produce a `fn fun_name` to name
```

Re-cut to the whole `let managed_recipe … ;` statement (557–567), which passed.

**Hang (R)**, which wedges the index daemon. The range was a bare block statement `{ … }`:

```rust
// svc_start_session_core.rs, lines 208–222: the range started on the `{`
        {
            let project_id = req.project_id.trim();
            if !project_id.is_empty() { … }
        }
```

```text
   indexing (+0ms): op 11 of 15: deep resolve ExtractMethod in `…/svc_start_session_core.rs`
   indexing (+4ms): starting rust-analyzer session
   indexing (+1ms): assist: ExtractMethod in this file
   indexing (+0ms): waiting for type inference at the anchor
```

After this there was no further output, and rust-analyzer, `tddy-index-daemon` and the client all
sat at 0% CPU. It happened three times: twice inside the full plan (the first time for over 11
minutes), and once with the op on its own, on a warm daemon that had just answered the block's
contents in 425 ms. That run never answered (400 s timeout), and **every later request to that
daemon queued behind it** until `./run-index-daemon --stop`. The wait needs a deadline, or a refusal for a range that starts on a block's `{`. The assist then left the
braces around the call it wrote (`{ self.provision_project_for_start(&req, os_user).await?; }`).
That builds and lints clean, so it was left as the engine wrote it.

### Plan `09c` (#524): K, P, Q, and a field init written out in full

Plan `09c` replaces plan `09`'s refused op 6 (`resolve_sandboxed_claude_worktree`, the whole
`match` over the worktree source, holding three `return`s). It uses four ranges between the exits:
the two halves of the `Project` arm (cut the worktree, then link its branch to the stack node), the
changeset write, and the project's default-branch lookup. It applied 4 of 4. The compile gate failed
on K alone: one `WorkflowRecipe`, qualified by hand.

- **P:** 2 comment lines lost ("A failed link never fails the spawn (D36) …"), restored.
- **Q:** `too_many_arguments` 13/7 and 14/7, `ptr_arg` ×3, and `unit_arg` in the link. Hand fix: a
  file-local `JailBranch<'a>` holds the branch-and-stack half of the request, and both functions take
  it (6 and 6 parameters).
- **A new Q shape, `redundant_field_names`:** where the moved code wrote `sessions_base: &sessions_base`
  and the extracted function receives `sessions_base` as a reference, the assist rewrote the field
  as `sessions_base: sessions_base` rather than the shorthand:

```rust
// before, in the handler
                    .resolve_chain_base_ref_status(&stack_parent::StackBaseLookup {
                        sessions_base: &sessions_base,
                        repo_root: &repo_root,
// after, in `create_jail_project_worktree(…, sessions_base: &PathBuf, …, repo_root: &PathBuf)`
                sessions_base: sessions_base,      // clippy::redundant_field_names
                repo_root: repo_root,              // clippy::redundant_field_names
```

### Item 4 of the 2026-09-24 run (#524): three more refusals, and a stale index

**R also hangs on a comment.** Plan `16`'s op 5 started its range on the line comment above the
statement (`// Re-wire managed-workflow orchestration …`). The check stopped at "waiting for type
inference at the anchor" and the daemon answered nothing more until it was restarted. Starting the
range on the `let` below passed. `extract_module` ranges that start on a doc comment or a section
banner (plans `11` and `12`) are unaffected, so the wait is `extract_method`'s alone. The same fix
applies to both shapes: a deadline, or a refusal for an anchor whose first token has no type.

**T — an anonymous lifetime reads as the `_` placeholder.** Plan `14`, three ops on
`spawn_claude_cli_session_inner`, refused at `check --deep` on a fresh daemon. Each signature is
fully typed. The one `_` in it is the `'_` of `SpawnStackParent<'_>`, which the check reads as the
E0121 placeholder:

```text
7: rust-analyzer's answer was unusable: rust-analyzer wrote `async fn chain_worktree_base_ref(sessions_base: &PathBuf, new_branch_name: &str, selected_integration_base_ref: &str, stack_parent: &super::SpawnStackParent<'_>, project_id: &str, repo_root: &PathBuf) -> Result<Option<String>, Status> {` — it produced the extraction before it could infer the types the signature needs, and `_` is not legal there (E0121). The crate graph was most likely still loading; retrying the operation against a warm server resolves it.
```

Ops 1 and 5 have the same shape. Every range that names `stack_parent` is refused this way, so
nothing that touches the spawn's PR-stack parent can be extracted from either CLI spawn
(`chain_base_ref`, `link_spawned_branch_without_failing_the_spawn`, the participant metadata). The
check should look for a type that *is* `_`, not for the character.

**U — rust-analyzer panics on one range.** Plan `16`'s op 3, `relaunch_sandboxed_runner`'s
`let mut runner_argv = vec![…];` plus the `if … { runner_argv.push(…) }` lines after it:

```text
3: plan is malformed: lsp: lsp server error -32603: request handler panicked: called `Option::unwrap()` on a `None` value
```

The panic is the server's. The engine reports it as a malformed plan, which it is not. It was
dropped from the plan.

**V — the warm daemon does not see a file edited outside a run.** After the DRY rows (hand edits),
plan `14` was refused with `project: _` and `projects_dir: Path` (unsized, by value). Both names come
from `find_registered_project`, a function DRY #3 had just added to `service_util.rs`. After
`./run-index-daemon --stop` and a cold start, the same plan got `project: ProjectData`, and op 6
passed. The server had answered from the tree as it was at its start. Plans `11`–`13` had passed
against the same stale server; they were re-checked on the fresh one before applying. Until the
daemon watches the tree, **restart it after any hand edit**, or its checks are about a tree that no
longer exists.

## Candidates, undecided

- **P:** carry the trivia with the moved nodes, or refuse an `extract_*` whose output has fewer
  comment lines than its input. A refusal is at least honest.
- **Q:** borrow `PathBuf`/`String`/`Vec<T>` locals as `&Path`/`&str`/`&[T]` in the generated
  signature, and drop a `mut` the callee does not need. Arity and tuple returns are the plan's
  problem, not the engine's. The plan should cut smaller ranges or introduce a parameter struct by
  hand afterwards.
- Either way, a lint gate after apply (see
  [the lint-gate TODO](./2026-09-24-restructure-apply-leaves-the-lint-gate-red.md)) would have
  reported Q, though not P.

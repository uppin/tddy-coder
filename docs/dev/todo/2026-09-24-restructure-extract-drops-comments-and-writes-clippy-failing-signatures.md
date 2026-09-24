# 2026-09-24 — `restructure` drops comments from the code it moves, and writes extract-method signatures clippy rejects

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), plans
`09b` (plan `09` without op 6) and `02-cli-session-manager-dir`, changeset
[`2026-09-23-carve-lifecycle-destructure`](../1-WIP/2026-09-23-carve-lifecycle-destructure.md)

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

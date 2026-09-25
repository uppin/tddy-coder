# 2026-09-24 — five `tddy-session-lifecycle` functions are still over 150 lines, and why each one stopped

**Category:** Future enhancement
**Source:** `#carve` 14/15, [#524](https://github.com/uppin/tddy-coder/pull/524), change history
[`2026-09-23-carve-lifecycle-destructure`](../changesets/2026-09-23-carve-lifecycle-destructure.md)
(the destructure's "Consent list", items 1–4)

The destructure brought the crate from 10 production functions over 150 lines to 5. It stopped at
these five because every remaining cut was refused by the engine, or needs the developer's consent
for a hand edit. Measured at `52e621a3` (fn line to closing brace, test blocks masked; the rule and
script are in the change history's "LoC assessment"):

| Function | File | Lines | Why it stopped | Section |
|---|---|---:|---|---|
| `start_sandboxed_cursor_cli_session` | `connection_service/svc_start_sandboxed_cursor_cli_session.rs:31` | 414 | DRY #1 needs coverage first | [the jail-launch TODO](./2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md) |
| `start_session_core` | `connection_service/svc_start_session_core.rs:51` | 358 | E4 refuses every range holding an early return | [E4](#e4--start_session_core-its-guards-are-refused-as-early-returns) |
| `start_sandboxed_claude_cli_session` | `connection_service/svc_start_sandboxed_claude_cli_session.rs:96` | 342 | DRY #1, and the argv range panics rust-analyzer (U) | [the jail-launch TODO](./2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md), [U](#u--the-runner-argv-ranges-stay-inline) |
| `spawn_claude_cli_session_inner` | `connection_service/claude_cli_spawn.rs:29` | 255 | T refuses every range naming the PR-stack parent | [T](#t--the-two-cli-spawns-stop-where-they-name-the-pr-stack-parent) |
| `spawn_cursor_cli_session_inner` | `cursor_cli_spawn.rs:23` | 244 | T, the same way | [T](#t--the-two-cli-spawns-stop-where-they-name-the-pr-stack-parent) |

`relaunch_sandboxed_runner` (149), `handle_rpc` (147) and `start_split_claude_cli_session` (146)
are just under the line.

## Why deferred

The developer's rules for the run (2026-09-24): *a refusal stops that operation, with no hand move
around it*, and hand edits are only the corrections that make an engine move build. Every item
below is either a refusal or a hand extract, so each needs the developer's consent, or an engine
fix. The developer's call at wrap (2026-09-24): "file the TODOs and move on".

## E4 — `start_session_core`: its guards are refused as early returns

**Why deferred:** needs developer consent (a hand extract). The engine is right to refuse: this is
#527's `refuse_early_returns` working as designed.

Plan `19` asked for four ranges, and `check --deep` refused all four, writing nothing. At
`52e621a3` they are:

| Range | Lines | What it is | Exits inside |
|---|---|---|---|
| agent allowlist check | 70–80 | the `if !agent_trim.is_empty() && agent_def.is_none() { … }` guard | 1 `return Err` |
| workspace branch | 203–322 | `if req.session_type.trim() == "workspace" { … }` | 3 `return Err`, 3 `return Ok` |
| claude-cli branch | 325–348 | `if req.session_type.trim() == "claude-cli" { … }` | 2 `return self.start_…().await` |
| cursor-cli branch | 351–375 | `if req.session_type.trim() == "cursor-cli" { … }` | 2 `return self.start_…().await` |

The refusal, verbatim (op 2, the workspace branch):

> 2: this seam cannot be cut here: the range returns early from the function around it, on line
> 221 (`return Err(Status {`) and line 284 (`return Err(status);`) and line 306
> (`return Err(status);`) and line 309 (`return Ok(started);`) and line 321
> (`return Ok(started);`). An extracted function cannot carry an early exit of its caller: the
> assist copies the `return` verbatim, so it returns from the new function instead — whose return
> type differs, which is `E0308` at best and a silently skipped exit at worst. Cut the range so it
> holds no `return`, or end it before the first one.

**What is left of the function is its exits.** `start_session_core` has **19** early exits
(lines 74, 93, 102, 129, 134, 147, 152, 182, 202, 221, 284, 306, 309, 321, 337, 347, 365, 374, 391):

- **9 are `return Err(…)` guards** (74, 93, 129, 134, 182, 221, 284, 306, 391). Each would extract
  soundly into a `Result<(), Status>` helper called with `?`.
- **10 are the dispatch's own exits**: six `return self.start_…().await` (147, 152, 337, 347, 365,
  374) and four `return Ok(…)` (102, 202, 309, 321). They end the function with a value, so they
  cannot move into a helper without changing the caller's control flow.

The engine cannot tell the two kinds apart, and it should not guess. A hand extract of the
`Err`-only guards is the option:

```rust
// before: svc_start_session_core.rs:128–137
        if req.session_type.trim() == "claude-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for claude-cli sessions",
            ));
        }
        if req.session_type.trim() == "cursor-cli" && req.model.trim().is_empty() {
            return Err(Status::invalid_argument(
                "model is required for cursor-cli sessions",
            ));
        }

// after, by hand: the guard returns a Result, and the caller keeps one `?`
        refuse_cli_start_without_a_model(&req)?;

fn refuse_cli_start_without_a_model(req: &StartSessionRequest) -> Result<(), Status> {
    let session_type = req.session_type.trim();
    if matches!(session_type, "claude-cli" | "cursor-cli") && req.model.trim().is_empty() {
        return Err(Status::invalid_argument(format!(
            "model is required for {session_type} sessions"
        )));
    }
    Ok(())
}
```

The workspace branch's `return Ok(started)` exits (309, 321) are the exception: they end the
branch, so the whole `if … { … }` body could become `return self.start_workspace_branch(…).await;`
by hand, as the claude-cli and cursor-cli branches already are.

**What would close it:** the developer's consent for a hand extract of the 9 guards (and optionally
the workspace branch body), reviewed as a hand edit, then `start_session_core` re-measured under
150. Or an engine operation that lifts a `return Err(e)`-only range into a `Result` helper and
rewrites the call with `?`; no such operation exists.

## T — the two CLI spawns stop where they name the PR-stack parent

**Why deferred:** engine gap. The engine side is already filed as **T** in
[the extract TODO](./2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md#item-4-of-the-2026-09-24-run-524-three-more-refusals-and-a-stale-index);
this section is the lifecycle side, the functions it blocks.

The inferred-placeholder check reads the anonymous lifetime `'_` in `SpawnStackParent<'_>` as the
E0121 placeholder `_`, so every range whose extracted signature takes the stack parent is refused.
Plan `14` dropped three ops on `spawn_claude_cli_session_inner` for this; plan `15` cut around them
on the Cursor side. Both functions take the parent by value:

```rust
// claude_cli_spawn.rs:45            stack_parent: stack_parent::SpawnStackParent<'_>,
// cursor_cli_spawn.rs:46            stack_parent: crate::connection_service::SpawnStackParent<'_>,
```

The ranges that stayed inline, in `spawn_claude_cli_session_inner` (the Cursor function has the same
three, at `cursor_cli_spawn.rs:107` and `:144`):

```rust
// claude_cli_spawn.rs:99–109 — refused op 7: the chain base
    let chain_base_ref = stack_parent
        .chain_base_ref(project_id, &sessions_base, &repo_root, new_branch_name,
                        selected_integration_base_ref)
        .await?;
    let worktree_base_ref =
        tddy_core::select_worktree_base_ref(selected_integration_base_ref, chain_base_ref);

// claude_cli_spawn.rs:147–149 — refused: the link that never fails the spawn (D36)
    stack_parent
        .link_spawned_branch_without_failing_the_spawn(&sessions_base, &spawned_branch, session_id)
        .await;
```

plus the participant-metadata block that passes `stack_parent: &stack_parent` (`claude_cli_spawn.rs:255`).

The refusal, verbatim (op 7):

```text
7: rust-analyzer's answer was unusable: rust-analyzer wrote `async fn chain_worktree_base_ref(sessions_base: &PathBuf, new_branch_name: &str, selected_integration_base_ref: &str, stack_parent: &super::SpawnStackParent<'_>, project_id: &str, repo_root: &PathBuf) -> Result<Option<String>, Status> {` — it produced the extraction before it could infer the types the signature needs, and `_` is not legal there (E0121). The crate graph was most likely still loading; retrying the operation against a warm server resolves it.
```

**What would close it:** the engine fix in the extract TODO (look for a type that *is* `_`, not
for the character), then plan `14`'s three refused ops and their Cursor equivalents re-run, and both
functions re-measured under 150. A hand extract is the alternative, with the developer's consent.

## U — the runner-argv ranges stay inline

**Why deferred:** engine gap (rust-analyzer panics). The engine side is **U** in
[the extract TODO](./2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md#item-4-of-the-2026-09-24-run-524-three-more-refusals-and-a-stale-index).

Plan `16` op 3 (`relaunch_sandboxed_runner`) and plan `18`'s argv op (the sandboxed Claude handler)
each asked rust-analyzer to extract the runner's argv, and the server panicked:

```text
3: plan is malformed: lsp: lsp server error -32603: request handler panicked: called `Option::unwrap()` on a `None` value
```

The range is a `let mut` vector literal followed by the conditional pushes that extend it:

```rust
// svc_relaunch_sandboxed_runner.rs:140–168 (trimmed); the same shape at
// svc_start_sandboxed_claude_cli_session.rs:338–375 and svc_start_sandboxed_cursor_cli_session.rs:268–294
        let mut runner_argv = vec![
            sandbox_runner_path,
            "--session-id".into(),
            session_id.to_string(),
            …
            "--stdio".into(),
        ];
        if resume {
            runner_argv.push("--resume".into());
        }
        if let Some(prompt_path) = &append_system_prompt_file {
            runner_argv.push("--append-system-prompt-file".into());
            runner_argv.push(prompt_path.to_string_lossy().to_string());
        }
```

`relaunch_sandboxed_runner` got under 150 without it; the Claude handler did not.

**What would close it:** a `runner_argv(…) -> Vec<String>` per caller, extracted by hand with the
developer's consent, or by the engine once U is diagnosed. The three argv builders are also a DRY
candidate for the shared jail launch, so the cheapest order is probably to do this as part of
[DRY #1](./2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md).

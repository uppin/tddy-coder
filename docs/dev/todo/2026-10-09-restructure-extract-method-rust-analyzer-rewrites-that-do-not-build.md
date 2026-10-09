# 2026-10-09 — three `extract_method` rewrites rust-analyzer writes that do not build

**Category:** Bug (upstream rewrites; the compile gate reports them)
**Source:** #reshape 4/19 (`extract-method-clean`), moved out of
`2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md` (plan `10b` on #524), which
#reshape 4 resolves and deletes

Plan `10b` on #524 applied 15 `extract_method`s, and after the import fix three build breaks remained, each fixed by hand:

```rust
// 1. a deref written before a macro statement (E0614: type `()` cannot be dereferenced)
            *log::warn!(
                "StartSession: could not remove session {session_id} after its \
                 jail could not be provisioned: {}",
// by hand: `log::warn!(`

// 2. a value passed by move while a reference into it is passed too (E0505)
        let repo_path = Path::new(&project.main_repo_path);
            .spawn_tool_session(req, progress, os_user, agent_def, livekit, project, repo_path)
// by hand: `&project`, and `repo_path` derived inside from the project

// 3. an expression range after `return` borrows what the callee consumes (E0308 ×2)
    async fn start_sandboxed_claude_cli_from_request(&self, …,
        sessions_base: &std::path::PathBuf,                 // the callee takes `PathBuf`
        managed_recipe: &Option<Arc<dyn WorkflowRecipe + 'static>>,   // the callee takes it by value
// by hand: by value
```

## Why deferred

None was reproduced against the dev shell's rust-analyzer (2026-03-30) during #reshape 4's discovery, and each is a
rewrite the assist chose; the compile gate already fails the run naming the error, so nothing lands silently. A lexical
repair (dropping a `*` before a macro path, say) would guess at the assist's intent.

## Next step

Reproduce each on a fixture crate with the dev shell's server. A reproduced one either gets a refusal before the edit
(the range shape is recognisable from the text) or an upstream rust-analyzer issue, linked here.

# 2026-10-06 — A run records every process it starts, through one recorder

**Type:** Feature

`git`, `cargo check`, `rustfmt` and the cold CLI's language server now go through `SpawnRecorder`,
which reports each process's start (argv, cwd, pid, env names) and its end (exit status or signal) to
a `SpawnObserver`. `JsonlSpawnRecord` appends the record as redacted JSONL; `ColdRunSpawnRecord`
(D1 = A″) defers the cold CLI's lines until `.restructure/` exists, so a refused run writes nothing.
`Options::spawns` and `RustBackend::with_spawn_recorder` are the wiring; the default discards.

`apply.rs`'s `ensure_git_worktree`/`git_output`/`run_git` take a `&SpawnRecorder`; `run_check` and
`run_rustfmt` receive it through the gate's arguments and `Tidying`.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-spawn-record.md).

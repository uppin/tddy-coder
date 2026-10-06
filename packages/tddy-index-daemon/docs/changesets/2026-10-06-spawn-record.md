# 2026-10-06 — The daemon records the language servers it starts

**Type:** Feature

`--spawn-record <PATH>` names an append-only JSONL file the daemon writes one record per line to. The
spawn observer rides inside the `LspRegistry` (`.with_spawn_observer`), and the same recorder reaches
the `Options` of check and apply, so a run's own `git`/`cargo`/`rustfmt` children land in the file
beside the language server it starts. The record is a sibling of the log rather than part of it — the
log is headered text a shell function rewrites, which JSONL cannot tolerate.

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-06-spawn-record.md).

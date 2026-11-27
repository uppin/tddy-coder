# 2026-11-29 — The workflow, changeset and task crates go through the Fs port

The production disk I/O of `tddy-workflow`, `tddy-workflow-engine`, `tddy-workflow-recipes`,
`tddy-changeset`, `tddy-tool-engine`, `tddy-task` and `tddy-actions` goes through the
`tddy-fs` port (refactor, no behaviour change): session-file formats, the artifact writer,
workflow hooks, the action cache, the start ledger and the contained file tools run behind
`<fn>_on` seams and injected handles with `DiskFs` only at composition roots. No `Fs` trait
method was added.

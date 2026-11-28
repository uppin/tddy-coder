# 2026-11-30 — The code tools go through the Fs port

The production disk I/O of `tddy-code-restructuring`, `tddy-code-analysis`,
`tddy-index-daemon`, `tddy-lsp`, `tddy-lsp-executor`, `tddy-semantic-index` and
`tddy-bsp` goes through the `tddy-fs` port (refactor, no behaviour change): the
restructure engine's run state, gates and coverage pipeline, the warm index and the
LSP executors run behind `<fn>_on` seams and injected handles with `DiskFs` only at
composition roots. No `Fs` trait method was added; the journal append's lost fsync is
recorded as a port-surface gap in the changeset.

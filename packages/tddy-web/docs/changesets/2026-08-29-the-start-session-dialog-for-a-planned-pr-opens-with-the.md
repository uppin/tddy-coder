# 2026-08-29 — the Start-session dialog for a planned PR opens with the orchestrator's documents attached

**Type:** Feature

`stackDocAttachments` mirrors the daemon's ordering and offers only documents the orchestrator's `contextDocs` reports as existing; `CreateSessionInitialValues` gains `attachments` and `useSessionAttachments` seeds them through a lazy initializer, so a row the operator removes stays removed. They render as ordinary attachment rows — a default, not an invariant — and what is left attached at submit is what is sent, each by reference to the orchestrator's own session. `HostDocumentPicker` now reads the server's `relativePath` through a shared `contextDocRelativePath`, keeping the old `kind`-based derivation only as a fallback, so the picker and the dialog cannot address one document two ways.

# 2026-10-18 — code_navigation.proto: CodeSession and index state
**Type:** Feature

`rpc CodeSession(stream CodeSessionRequest) returns (stream CodeSessionEvent)` with `OpenCodeSession`,
`CodeQuery` (`CodeQueryKind` hover / definition / references / probe), `CancelCodeQuery`,
`CodeQueryHeld`, `CodeQueryAnswer`, `ProbeAnswer` / `Definability`; `CodeIndexState` (`IDLE`, `LOADING`,
`READY`, `FAILED`, `NONE`) and `CodeIndexPhase` on `CodeIndexProgress`; `state` on `DefinitionResponse`,
`ReferencesResponse`, `HoverResponse`. TS bindings regenerated in `tddy-web/src/gen` and
`tddy-rust-typescript-tests/gen`.

Docs: [code-navigation-proto.md](../code-navigation-proto.md#the-code-session). Cross-package entry:
[2026-10-18-code-index-warm-on-demand.md](../../../../docs/dev/changesets/2026-10-18-code-index-warm-on-demand.md).

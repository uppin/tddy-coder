# 2026-11-21 — An orchestrator queries the LSP of any tddy worktree

Orchestrator sessions are offered `LspDiagnostics`, `LspDefinition`, `LspReferences`, `LspHover` and
`LspSymbols` with an optional `root`: any worktree in their read reach, or a root they added.
`AddLspRoot` adds a root and starts loading its index — one outside the reach is asked of the operator
first, like `RequestPathAccess`; `RemoveLspRoot` drops a root the session added without asking;
`ListLspRoots` lists them. See [Project concept § LSP roots](../project-concept.md#lsp-roots).

# 2026-09-09 — Host and worktree calls move to their own services

**Type:** Architecture

Root node of the `#unbundle` stack ([#470](https://github.com/uppin/tddy-coder/pull/470)). Full
story in the cross-package entry: [2026-09-09-unbundle-host-worktree-services.md](../../../../docs/dev/changesets/2026-09-09-unbundle-host-worktree-services.md).

Every host and worktree call site now reaches `host.HostService` or `worktree.WorktreeService`
instead of `connection.ConnectionService`. **The transport layer did not change**:
`useHttpClient(service)` and `clientFor<S extends DescService>(service)` were already
service-generic, so what moved is import paths, call sites and the six hard-coded
`ConnectionService` bindings in `src/rpc/`.

`src/gen/` was regenerated — `connection_pb.ts` shrinks by 4,126 lines alongside
`tddy-rust-typescript-tests/gen/connection_pb.ts` — and the 736-line Cypress
`connectionServiceBackend.ts` fake splits per service. The generated TypeScript stays in
`src/gen/`: a workspace package per generated service would need a root `workspaces` entry, a
`workspace:*` entry, a fresh `bun install` and regenerated lockfiles for no gain, since
`buf generate` already produces one `*_pb.ts` per proto with no config change.

Suites keep `mountWithRpc` + `anInMemoryRpcBackend`; `cy.intercept` is not used.
`WorktreesAppPage.cy.tsx` pins the three-way split the screen now makes — the worktree feed under
`WorktreeService`, the host roster under `HostService`, and `ListProjects` still under
`ConnectionService`. 231 specs, 1419/1419, 5m34s; unit 1178/0.

**A web bundle from before this change cannot talk to a daemon from after it**, because 17 method
coordinates moved. That is the licence this stack was given, and it matches the repo's policy of
breaking freely and migrating every consumer in the same change.

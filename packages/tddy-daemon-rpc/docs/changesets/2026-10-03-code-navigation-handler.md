# 2026-10-03 — `CodeNavigationServiceImpl` is served from this crate

**Type:** Architecture

`#live-plan` 8/15, PR [#574](https://github.com/uppin/tddy-coder/pull/574). Cross-package entry:
[2026-10-03-code-navigation.md](../../../../docs/dev/changesets/2026-10-03-code-navigation.md).

The code-navigation handler implements RPC methods, which `tddy-daemon`'s `unbundle_endpoint` suite
keeps out of the daemon, so it lives here. `IndexChannelSource` is the port through which it dials the
index daemon; `tddy-daemon` implements it for `IndexDaemonRegistry` and registers the entry in
`runtime.rs`. `tddy-index-daemon` and `tonic` (transport) are new dependencies of this crate; in
`tddy-daemon`, `tddy-index-daemon` is a dev-dependency again. Behaviour is unchanged: the same
authorisation, errors and location mapping, pinned by `tddy-daemon`'s `code_navigation_acceptance`
(7) and this crate's four unit tests.

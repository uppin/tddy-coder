# 2026-11-16 — A derived host, named by id, resolved for a caller

**Type:** Feature · `#remote-shell` 11/11, PR [#927](https://github.com/uppin/tddy-coder/pull/927)

`HostServiceImpl::resolve_derived_host` (`service/derived_host_resolution.rs`) lists the caller's derived hosts and resolves one id to its parent daemon and `SessionTarget`; unknown ids are refused naming the listed ones and unusable hosts with their reason (`DerivedHostResolutionError`). See the cross-package entry in `docs/dev/changesets/2026-11-16-remote-shell-orchestrator-target-start.md`.

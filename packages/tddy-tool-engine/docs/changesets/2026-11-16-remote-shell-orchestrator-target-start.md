# 2026-11-16 — The start tool names a derived host with `target`

**Type:** Feature · `#remote-shell` 11/11, PR [#927](https://github.com/uppin/tddy-coder/pull/927)

`StartSessionArgs.target` carries a derived-host id from `DescribeSessionStartOptions.derived_hosts`; `target` leaves `INTERNAL_START_FIELDS` and is offered by the schema, whose `ssh_config_host` text states that a wildcard `Host` is not an alias. `StartOptions` gains `derived_hosts` (`DerivedHostChoice`) and `derived_hosts_unavailable`. The remote-placement offers live in `offer_remote_placement`, keeping `schema_offering` at 192 lines under its recorded 195. See the cross-package entry in `docs/dev/changesets/2026-11-16-remote-shell-orchestrator-target-start.md`.

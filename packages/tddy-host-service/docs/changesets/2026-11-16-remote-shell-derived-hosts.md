# 2026-11-16 — SSH hosts as hosts derived from a daemon

**Type:** Feature · `#remote-shell` 5/7, PR [#919](https://github.com/uppin/tddy-coder/pull/919)

`ListDerivedHosts` answers the hosts this daemon reaches through a remote shell. `src/derived_hosts.rs` holds the `DerivedHostSource` port and `SshConfigDerivedHosts`, which lists the explicit aliases of the mapped OS user's `~/.ssh/config` as hosts of kind `SSH`, identified `<parent_instance_id>/ssh/<alias>` and targeted by `SshTarget { config_host: alias }`. The handler (`src/service/derived_hosts.rs`) routes by `daemon_instance_id`, asks every registered source (`with_derived_host_source` adds kinds after the SSH one), and answers `FAILED` with the first failure's reason while still carrying the hosts that listed; a daemon that reaches none is `Ok` and empty. See [host-service.md](../host-service.md).

Verification: `./test -p tddy-host-service` — `derived_hosts_acceptance` 4/4, handler tests 2/2, `derived_hosts` unit tests 7/7; nix clippy `--all-targets -D warnings` and `cargo fmt --check` clean.

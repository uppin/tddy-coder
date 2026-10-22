# 2026-10-22 — cli_harnesses config map

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)). `DaemonConfig.cli_harnesses: BTreeMap<String, CliHarnessConfig { binary_path }>` keyed by
session type, and `DaemonConfig::cli_harness_binary(session_type, default_binary, binary_env)`
(environment, then config, then default; blank counts as unset). See [daemon kernel](../daemon-kernel.md).

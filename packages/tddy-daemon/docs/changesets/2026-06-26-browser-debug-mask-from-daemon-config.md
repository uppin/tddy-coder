# 2026-06-26 — Browser DEBUG mask from daemon config

**Type:** Feature

`DaemonConfig.debug: Option<String>` (`#[serde(default)]`); `run_server` gains `debug` param, set on `ClientConfig.debug`; `main.rs` reads `config.debug` → `web_debug` → `run_server`; `dev.daemon.yaml` ships `debug: "tddy:term:*"`; 3 unit tests (default/absent-yaml/parse). PR [#233](https://github.com/uppin/tddy-coder/pull/233). (tddy-daemon)

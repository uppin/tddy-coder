# 2026-06-26 — **ClientConfig.debug

**Type:** Fix

serve browser DEBUG mask** — `ClientConfig.debug: Option<String>` (`skip_serializing_if = "Option::is_none"`), omitted in standalone CLI mode (`build_client_config` sets `debug: None`); 2 unit tests (omitted when `None`, serialized when `Some`). PR [#233](https://github.com/uppin/tddy-coder/pull/233). (tddy-coder)

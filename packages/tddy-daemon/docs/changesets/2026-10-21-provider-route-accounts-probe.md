# 2026-10-21 — HTTP model probe for provider accounts

**Type:** Feature

`#provider-route` 1/9 ([#662](https://github.com/uppin/tddy-coder/pull/662)). New
`provider_model_probe::HttpProviderModelProbe`: `GET <openai root>/models` with a bearer key, or
`GET <anthropic root>/v1/models` with `x-api-key` for an Anthropic-only endpoint, 20 s timeout, the
first 300 characters of a refusal body passed back. `account_linking::accounts_service_over` wires
it, with the vault store, as the service's provider-account half. See
[daemon endpoint](../daemon-endpoint.md).

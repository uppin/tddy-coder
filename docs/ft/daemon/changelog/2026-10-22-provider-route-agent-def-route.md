# 2026-10-22 — Harness profiles

An agent definition in `<tddyhome>/agents/*.yaml` can be a **harness profile** — `harness:`,
`provider_account:` (label or id), `model:` — offered in the agent list and started as that harness on
that account and model. It fills the agent-definition level of route precedence: a session-level choice
beats it, it beats the person's default. A profile with `base_url`/`api_key`, an unknown harness, an
account the person does not hold, or a label two accounts share is refused. YAML only, no UI editor. See
[Harness profiles](../harness-profiles.md).

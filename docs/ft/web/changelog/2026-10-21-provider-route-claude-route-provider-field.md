# 2026-10-21 — Provider field on the New-session form

The New-session form shows a **Provider** field for CLI sessions: "Anthropic (built in)" by default,
or one of the person's provider accounts the daemon says the harness can run on. Choosing an account
lists that account's models. For Cursor CLI the field shows why Cursor cannot run on another
provider. See [tool-session model selection § Provider](../tool-session-model-selection.md#provider).

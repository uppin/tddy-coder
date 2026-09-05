# 2026-08-16 — Models & Agents

- **A new Models & Agents entry (`#/models`) lists every model offered by every provider configured on every connected daemon**, with its owning daemon, capability labels and whether it is resident — the fleet view that previously meant shelling into each host and running `ollama ps`. See [models-and-agents.md](../models-and-agents.md).
- **Models can be loaded and unloaded from the browser**, routed to the model's owning daemon rather than the selected one; a cloud model reports residency as unsupported and is offered neither action.
- **Any `llm`-labelled model can be chatted with over ACP**, reusing the existing chat transport. Labels are only ever derived from what a provider reports — Ollama and Fireworks describe their models, OpenAI's `/v1/models` does not, so its models carry no label and are offered no chat rather than being guessed at.
- **Assistants compose a model with a system prompt and tools**, and become selectable as `--agent <name>` when starting a session. An assistant's tools run confined to a workspace the operator picks from that daemon's own projects.
- **Providers are added explicitly through the UI, never auto-detected**, and their API keys never leave the daemon — responses carry only whether a credential is stored.
- **Everyone sees the fleet; only a row's owner may change it.** One operator cannot edit, delete, or use another's provider credentials.
- **One daemon failing costs one row, not the page** — the four reads per daemon degrade independently, and "not connected", "loading", "read failed" and "no providers yet" stay visually distinct.
- **Known limitations:** a provider cannot be edited (delete and recreate); a missing credential surfaces as the provider's own 401 rather than our own error; and an assistant's tools, while path-confined, run as the daemon user rather than the caller's. Tracked in [docs/dev/TODO.md](../../../dev/TODO.md).

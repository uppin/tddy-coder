# Validate Production Readiness Report

## Summary

The ListAgents / `allowed_agents` feature is **functionally sound** for production: YAML-backed allowlist, a single mapping path for labels (`agent_list_mapping`), server-side enforcement on `StartSession` when the list is non-empty, and web UI driven by RPC with tests at unit, integration, and component levels. **Gaps** are mainly operational: coupled `Promise.all` for ListTools + ListAgents (version skew and partial-failure UX), verbose **info-level** daemon logging per allowlist row, **browser `console.debug`** left in place, **empty `allowed_agents` disables** StartSession agent checks (open allowlist), and **operator documentation** is still expected via the docs changeset workflow rather than being present in-repo here.

---

## Strengths

- **Security (server-side allowlist):** When `allowed_agents` is non-empty, `StartSession` rejects unknown agent ids with `invalid_argument` and a clear configuration hint, so the UI cannot bypass policy by crafting requests.

- **Configuration / schema:** `DaemonConfig` and `AllowedAgent` use `#[serde(deny_unknown_fields)]`, reducing silent misconfiguration. `allowed_agents` defaults to an empty vec; optional `label` mirrors `allowed_tools` patterns. `dev.daemon.yaml` documents intent and lists four backends with human-readable labels.

- **Consistency:** `list_agents` and label rules are centralized in `agent_list_mapping::agent_allowlist_rows`, with unit tests for label trim and blank-label fallback—aligned with `list_tools` label behavior.

- **Error handling (daemon):** `list_agents` is a simple in-memory map; no panics on expected paths. `StartSession` agent validation runs after auth/OS-user resolution and before spawn work.

- **Performance (N small):** Two short RPCs from the web client and O(n) scans over a small allowlist on the daemon are appropriate; no unnecessary blocking or fan-out.

- **Test coverage:** Rust unit tests on mapping; acceptance tests for config, ListAgents echo, ListTools regression, and unknown agent on StartSession; Bun tests for `agentOptions`; Cypress stubs for ListAgents.

---

## Gaps / Risks

- **Deployment / version coupling (high operational impact):** `ConnectionScreen` loads tools and agents via `Promise.all([listTools, listAgents])`. A daemon or proxy that does not implement `ListAgents` causes **both** lists to clear and a single generic error—stricter than prior ListTools-only behavior. **Mitigation in ops:** ship daemon + web together; document minimum compatible versions.

- **Security semantics when allowlist is empty:** If `allowed_agents` is empty, `StartSession` does **not** restrict `agent` (only non-empty agent strings are checked against the list). Operators who omit the key get “any agent id accepted,” which may be surprising for a feature marketed as an allowlist.

- **Observability / logging:** `agent_allowlist_rows` logs **`log::info!` per row** (id + display label). At default info levels and moderate N, this is noisy and may leak deployment choices into logs. `list_agents` also logs an info line with agent count (reasonable).

- **Client logging:** `ConnectionScreen` and `agentOptions.ts` use **`console.debug`** for load counts and coalescing—acceptable for dev but worth removing or gating before treating the bundle as production-polished.

- **UI error state:** On combined load failure, both `tools` and `agents` are cleared and `error` is set; there is **no dedicated retry** for that initial fetch (user must refresh or re-auth). The error message does not distinguish ListTools vs ListAgents failure.

- **Configuration validation:** No daemon-side check for **duplicate `id`** values in `allowed_agents`; duplicates would still enforce membership but could confuse operators and UIs.

- **Hygiene / docs:** Untracked `.tddy-red-*` artifacts and Cypress `MOCK_DEFAULT_LIST_AGENTS` vs `dev.daemon.yaml` drift risk were noted in the evaluation report; operator-facing `allowed_agents` documentation belongs in the docs workflow before release.

---

## Recommendations (prioritized)

1. **Release / ops:** Document **co-deploy** requirements for tddy-web and tddy-daemon (or minimum daemon version) whenever `ListAgents` is required; optionally add a **changelog or compatibility matrix** entry for Connection API consumers.

2. **Observability:** Downgrade per-row mapping logs in `agent_list_mapping.rs` from **`info` to `debug`** (keep a single summary at info or debug only). Align with production log level expectations.

3. **Client polish:** Remove or feature-gate **`console.debug`** in `ConnectionScreen.tsx` and `agentOptions.ts` for production builds if the team wants a clean console.

4. **Resilience (optional product decision):** If backward compatibility with older daemons matters, consider **independent** ListTools / ListAgents requests (or graceful fallback when `ListAgents` returns unimplemented) instead of `Promise.all`—only if product accepts the added complexity.

5. **Operator clarity:** In operator docs, state explicitly that **empty `allowed_agents` means no agent id restriction** on `StartSession`; recommend non-empty allowlists for locked-down deployments.

6. **Schema hardening (optional):** Add startup or load-time validation for **duplicate agent ids**; reject or warn in logs with a clear message.

7. **Hygiene:** Delete or **gitignore** `.tddy-red-capture.txt` / `.tddy-red-submit.json` before merge; add a one-line comment or doc link tying Cypress default ListAgents stubs to `dev.daemon.yaml` to reduce drift.

---

*Scope: read-only review of paths listed in the validation request; aligned with `evaluation-report.md` where applicable.*

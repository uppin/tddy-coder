# Validate prod-ready report — GitHub PR tools refactor

## Summary

Production code for GitHub PR MCP tools (`github_pr.rs`, `server.rs`) follows a consistent error model (`GithubPrError`), avoids logging secrets, and returns MCP JSON with `{ "error": "<message>" }` or success payloads without embedding raw API bodies in user-facing errors. Configuration matches project conventions: `GITHUB_TOKEN` preferred over `GH_TOKEN`, empty values ignored, and live REST calls documented as requiring `curl` on `PATH`. **Merge-pr hooks** gate supplemental prompt text on actual token presence; **tdd-small merged red** always appends a static GitHub-tools awareness blurb (no env gating)—a behavioral nuance, not a secret leak.

**Overall:** Suitable for production use with the same operational assumptions as the evaluation report (curl + PATH dependency, duplicated token checks vs `merge_pr/github.rs`). Residual risks are mostly **process visibility of the Bearer token** when spawning `curl`, and **blocking I/O** on the MCP thread—acceptable for low-frequency PR operations if documented.

---

## Findings (by category)

### Error handling

| Area | Assessment |
|------|------------|
| **`GithubPrError`** | `AuthenticationRequired` has a fixed, safe message. `Rest(String)` documents that messages must not include token material; current `Display` paths use status codes, parse errors, or generic I/O text—no token concatenation. |
| **HTTP non-success** | Create/update paths treat non-2xx as `Rest` with message **only** `HTTP {status}`—API error JSON from GitHub is **not** copied into errors or logs (only `body_len` in debug on failure path for create). |
| **Parse failures** | JSON parse errors use `serde_json` error text (no response body echoed in full in the error string for the common case—create path returns structured parse error). |
| **`curl_github_json` failures** | Spawn failure includes `method` and `url` (no token in URL). Non-zero exit includes stderr via `String::from_utf8_lossy`—risk of noisy or unusual stderr is low; tokens are not passed on stderr by design. |
| **MCP handlers (`server.rs`)** | Success: JSON `{ "pull_number": n }` or `{ "ok": true }`. Failure: `{ "error": msg }` where `msg` is `format!("{e}")` from `GithubPrError`—safe given enum variants. |

### Logging

| Location | Content | Token risk |
|----------|---------|------------|
| `github_pr.rs` | `info!` / `debug!` with `owner`, `repo`, `pull_number`, `header_keys` only (not header values) | Low |
| | Messages like "no GITHUB_TOKEN/GH_TOKEN" | None (variable names only) |
| `server.rs` | Same identifiers; `debug!` on error with full error string | Low if `GithubPrError` stays constrained |
| `merge_pr/hooks.rs` | `task_id`, repo path, merge_base, branch name in logs | No secrets |
| `tdd_small/red.rs` | `awareness.len()` in `info!` | None |

No log line prints token values, `Authorization` header values, or full GitHub response bodies.

### Configuration

- **`GITHUB_TOKEN` / `GH_TOKEN`:** `github_token_from_env()` prefers `GITHUB_TOKEN`, then `GH_TOKEN`, trimming empty strings—aligned with merge-pr awareness checks (`hooks.rs` duplicates the boolean “present” logic via `github_env_token_present()`).
- **`curl`:** Documented in tool descriptions and module comment; `Command::new("curl")` uses `PATH` resolution—no bundled binary. Failure is explicit (`GithubPrError::Rest`).

### Security

| Topic | Notes |
|-------|--------|
| **Tokens in logs** | Not observed. |
| **Headers** | REST uses `Bearer`, `Accept`, `User-Agent`, `X-GitHub-Api-Version` consistent with GitHub REST guidance. Mock transport records full headers for tests—test-only. |
| **API responses** | Success paths parse minimal fields (`number` on create). Error paths avoid returning raw GitHub JSON to the agent—reduces accidental leakage of internal messages, at the cost of less detailed diagnostics. |
| **Subprocess / `ps` visibility** | The Bearer token is passed as a **command-line argument** to `curl` (`-H "Authorization: Bearer …"`). On shared hosts, process arguments may be visible to other users—**inherent limitation** of the current design; not unique to logging. |
| **Injection** | `Command` uses argument list (no shell); URL is built from `owner`/`repo`/number—no shell interpolation. |

### Performance

- **`curl_github_json`** uses `std::process::Command::output()`—**blocking** the calling async/runtime thread for the duration of the HTTP request.
- MCP tool handlers run this synchronously inside `github_create_pull_request` / `github_update_pull_request`. For occasional PR create/update calls this is **acceptable**; under high concurrency or strict latency SLOs, consider `spawn_blocking` or an async HTTP client (would be a larger refactor).

---

## Risks

1. **Token visibility via process list** — Bearer token in `curl` argv may be visible to same-machine observers; mitigations would be non-curl transport (e.g. `reqwest` with in-memory headers) or `curl` features that avoid argv exposure if available on target platforms.
2. **Opaque HTTP errors** — Agents see status-only messages on 4xx/5xx; debugging production failures may require enabling debug logs or reproducing with `curl` manually (still no automatic body logging today).
3. **Blocking I/O** — Can stall the MCP handler thread during slow GitHub responses; acceptable for typical PR workflows, not for bulk automation without tuning.
4. **Duplicated env logic** — `github_env_token_present()` in `hooks.rs` vs `github_token_from_env()` in `github_pr.rs` could drift; low security impact, maintenance risk.
5. **Prompt consistency** — Merge-pr appends GitHub MCP awareness only when a token is present; tdd-small merged red **always** appends a static “when authenticated…” line. No secret exposure, but UX/docs may need to clarify.

---

## Recommendations

1. **Document** in operator-facing docs: require `curl` on `PATH`, document `GITHUB_TOKEN`/`GH_TOKEN`, and note blocking behavior and optional process-visibility concern for high-security environments.
2. **Consider** centralizing token presence checks (shared helper or crate) to match `github_token_from_env` semantics everywhere.
3. **Optional hardening:** replace subprocess `curl` with a Rust HTTP client to keep secrets off argv and to enable non-blocking execution patterns; only if product requirements justify the dependency and refactor.
4. **Optional:** for operational debugging without logging full bodies, structured `debug!` of GitHub `message` field from error JSON only (still no raw dump)—evaluate against “no leakage” policy.
5. **Align** tdd-small vs merge-pr prompt gating if product intent is “only mention MCP PR tools when token is set”—currently tdd-small is unconditional static text.

---

*Based on review of `plans/evaluation-report.md` and production sources: `packages/tddy-tools/src/github_pr.rs`, `packages/tddy-tools/src/server.rs`, `packages/tddy-workflow-recipes/src/merge_pr/hooks.rs`, `packages/tddy-workflow-recipes/src/tdd_small/red.rs`.*

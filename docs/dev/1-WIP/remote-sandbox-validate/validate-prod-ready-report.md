# Validate prod-ready — Remote sandbox

## Summary

This review covers the new remote sandbox surface: `RemoteSandboxService` (gRPC/Connect), daemon registration, `tddy-remote` CLI (Connect client, rsync RSH bridge), and daemon VFS helpers. The feature delivers per-session temp roots, relative-path sanitization for VFS RPCs, loopback-only rsync TCP bridging, and poll-based byte forwarding in `tddy-remote` for rsync.

**Overall:** Functionally aligned with the evaluation report’s “medium” risk: the RPCs are **high capability** (arbitrary exec, file write, rsync server) and are registered on the same Connect router as other daemon services. Production readiness **requires** an explicit authorization and abuse model (who may call these methods, rate limits, and network exposure). Several **reliability and DoS** gaps remain: unbounded message payloads, registry growth without eviction, and a **panic** path when creating session directories fails.

**Clippy:** `./dev cargo clippy -p tddy-service -p tddy-daemon -p tddy-remote -- -D warnings` was run; it **failed** on `tddy-service` (see [Tooling / clippy](#tooling--clippy)). No code changes were made as part of this review.

---

## Findings

### Critical

- **None labeled “critical” in isolation** — the highest risk is **authorization**: unrestricted use of `ExecNonInteractive` / `PutObject` / `OpenRsyncSession` by any caller who can reach the Connect endpoint is equivalent to remote code execution and arbitrary writes under the daemon process. This must be decided at the product/network/auth layer; the code does not implement per-method authz.

### High

- **[High] Authorization / attack surface** — `RemoteSandboxService` is always registered when the daemon starts (`packages/tddy-daemon/src/main.rs`). There is no service-specific check in the reviewed code tying RPCs to a GitHub session or connection identity. Any client that can POST Connect unary calls to `/rpc/remote_sandbox.v1.RemoteSandboxService/*` at the daemon’s web port can invoke exec/VFS/rsync (subject to whatever global HTTP exposure and TLS the bundle uses). Aligns with `evaluation-report.md` “medium/security” item; treat as **high** for any Internet-facing or untrusted-network deployment until explicitly gated.

- **[High] Arbitrary command execution** — `ExecNonInteractive` runs `argv[0]` with arguments under the session root (`remote_sandbox_service.rs`). This is powerful by design but must be restricted to trusted callers.

### Medium

- **[Medium] Panic on session root creation failure** — `SandboxRegistry::root_for_session` uses `panic!` when `create_dir_all` fails (`unwrap_or_else`). A full disk or permission error can **crash the daemon** instead of returning `Status::internal`.

- **[Medium] No resource limits on RPC payloads** — `PutObjectRequest.content` and `ExecNonInteractiveResponse.stdout` are unbounded in the API; `Command::output().await` buffers full stdout in memory. A malicious or buggy peer can cause **memory pressure** or OOM.

- **[Medium] Session registry never evicts** — `HashMap<String, PathBuf>` grows per distinct `session` key with no TTL or cap; temp dirs accumulate on disk until process exit. Long-running daemons risk **disk exhaustion**.

- **[Medium] Symlink / VFS “escape” via filesystem** — Path sanitization rejects `..` and absolute paths but does not prevent operations from following **symlinks** already present under the session root (e.g. created by a prior `exec` or rsync). Writes to `root.join(rel)` may still resolve outside the intended tree via symlinked path components. The comment in `vfs.rs` (“symlink-safe”) is stronger than what the string-only check guarantees.

- **[Medium] Rsync client bridge: unbounded pending buffers** — In `rsh.rs`, `pending_up` / `pending_down` can grow without cap if one direction stalls; **memory growth** is possible under pathological flow control.

### Low

- **[Low] `ExecChecksum` ignores input** — Request message is empty in proto; implementation ignores request and runs a fixed shell snippet. Fine for smoke tests but misleading if treated as a general checksum API.

- **[Low] Empty `session` behavior** — `ExecNonInteractive` maps empty session to `"default"`; `PutObject` / `StatObject` / `OpenRsyncSession` use `root_for_session(&r.session)` with no special case, so empty string is a distinct bucket from `"default"`. **Inconsistent session semantics** can confuse callers and tests.

- **[Low] Logging** — Logs include `session`, paths, program name (`argv[0]`), and rsync bridge port/fd info. No obvious tokens in reviewed paths; avoid logging full `argv_json` or file contents (currently not logged). **Connect base URL** is logged at info when resolving authority (`config.rs`) — ensure internal URLs are acceptable in log sinks.

- **[Low] Client config** — `default_authority` in YAML is parsed but unused (`evaluation-report.md`); reduces clarity for operators.

- **[Low] Hygiene** — Tracked or stray `.red-remote-sandbox-test-output.txt` (per evaluation report) should be removed or gitignored.

- **[Low] Blocking rsync bridge** — `run_rsync_server_bridge_blocking` correctly runs in `spawn_blocking`; acceptable pattern. `OpenRsyncSession` accepts one TCP connection then runs rsync; a second connect goes to a listener that may still be waiting or idle — edge cases only matter for misuse.

---

## Recommendations

1. **Define and enforce authorization** for `remote_sandbox.v1.RemoteSandboxService`: e.g. require authenticated Connect context, restrict to localhost or VPN, or separate listener — **product decision**, not only code.

2. **Replace panic** in `root_for_session` with **recoverable errors** propagated to RPC handlers (`Status::internal` or `resource_exhausted`).

3. **Add limits:** max `PutObject` size, max `ExecNonInteractive` stdout (truncate or stream), max concurrent `OpenRsyncSession` / per-session ops, and **session TTL or explicit cleanup** API.

4. **Symlink policy:** document that the sandbox is not a hardened container; optionally use `openat`-style operations or refuse writes when path components are symlinks, if threat model requires it.

5. **Cap `pending_*` queues** in the rsync poll bridge or document best-effort memory limits.

6. **Align empty session** handling across all RPCs (either always default or always reject empty).

7. **Codegen / clippy:** allow-list generated `remote_sandbox.v1.rs` lints or adjust codegen so `cargo clippy -D warnings` passes; add `Default` for `SandboxRegistry` or `#[allow]` with rationale (see clippy output below).

8. **Remove or gitignore** stray test output files; wire `default_authority` or remove the field until used.

---

## Open questions

- **Who is allowed to call** `RemoteSandboxService` in production (same as web UI users, API keys, or internal only)?

- Should these RPCs be **disabled by default** in daemon config until explicitly enabled?

- **TLS and trust:** `tddy-remote` uses `reqwest` with a configurable base URL; is the expected deployment always HTTPS with pinned or system trust?

- **Multi-tenant isolation:** Is one OS process / one daemon instance per tenant acceptable, or is stronger isolation (containers, separate users) required?

- **Retention:** How long should session sandboxes live, and who triggers deletion?

---

## Tooling / clippy

Command: `./dev cargo clippy -p tddy-service -p tddy-daemon -p tddy-remote -- -D warnings`

**Result: failed** (stopped in `tddy-service`).

Observed issues:

- Generated `target/debug/build/.../out/remote_sandbox.v1.rs`: unused imports (`futures_util::Stream`, `StreamExt`, `tokio::sync::mpsc`), unused variable `method` in `is_bidi_stream` (matches evaluation report “Generated remote_sandbox.v1.rs warnings”).
- `packages/tddy-service/src/remote_sandbox_service.rs`: `clippy::new_without_default` on `SandboxRegistry`.

Re-run after addressing codegen or lint attributes; `tddy-daemon` / `tddy-remote` may have additional findings once `tddy-service` compiles cleanly under `-D warnings`.

---

## Reference alignment

This report extends [evaluation-report.md](./evaluation-report.md): same security theme, adds concrete code-level findings (panic, limits, symlink nuance, rsync buffers, session semantics) and records clippy failure details.

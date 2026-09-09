# 2026-09-09 — `tddy-tools restructure` could not obtain a single code action

**Type:** Fix
**Packages:** `tddy-lsp`, `tddy-code-restructuring`, `tddy-tools`

Every `restructure` operation that needs a rust-analyzer assist — `extract_method`,
`extract_module`, `extract_trait`, `extract_module_to_file`, `inline_method`, which is all of them
but `rename_symbol` — failed on every input with:

```
plan is malformed: rust-analyzer offers no "extract into function" assist for the given range
```

The message was false. rust-analyzer was returning an **empty** code-action list for every range,
because the client had told it nothing about itself.

## What was wrong

Four defects, all on the **bridge** transport — the path `tddy-tools restructure` uses, where
`RustBackend` talks through a shared `tddy-lsp` client instead of spawning rust-analyzer itself.
`RustBackend::start` returns early when a bridge is present, and everything the self-spawned path
does in its handshake was on the other side of that early return.

**D4, the blocker.** `LspClient::initialize` sent `"capabilities": {}`, hardcoded. The backend's
`client_capabilities()` — with its comments explaining why snippet edits are withheld and
hierarchical symbols requested — was only ever sent on the self-spawned path. rust-analyzer returns
no code actions at all to a client that advertised no `codeAction` support, which is
indistinguishable from a range that supports no refactoring.

The same omission dropped `"general": { "positionEncodings": ["utf-8"] }`, leaving the server on the
LSP default of **utf-16** while this client counts **bytes**. The two agree on every line until one
carries a character outside ASCII, and then every column is wrong — silently. The self-spawned path
refuses that mismatch outright; the bridge never looked. `connection_service.rs`, the file this was
being built for, has em dashes throughout its comments.

**D1.** The response dispatcher read `result` and defaulted to `Value::Null`, so a JSON-RPC `error`
response was delivered as a *successful empty answer*. rust-analyzer's `ContentModified` — which
means "ask again", and which the backend has a whole retry loop for — arrived as "the server had
nothing to offer", leaving `request_settled` dead on this path. Any genuine server error was
swallowed the same way.

**D2.** `map_lsp_error` mapped every failure to `MalformedPlan`, timeouts included. A slow index was
reported as a defective plan, sending the reader to rewrite anchors that were never wrong — the
opposite of what `SKILL.md` says ("indexing timeout is not a plan defect"). Because only
`ServerCatchingUp` is retried, the misclassification also skipped the loops that exist to wait.

**D3.** Every request was capped at a hardcoded 10s that `--indexing-budget` could not reach, so the
documented remedy for a slow machine was inert. A `codeAction` against a cold index does not answer
in ten seconds.

## What changed

- `LspError::Server { code, message }`; the pending request is handed the server's error instead of a
  null result.
- `LaunchSpec::with_capabilities` / `with_initialization_options`, threaded through
  `LspClient::initialize`. `tddy-tools` supplies the restructure backend's own
  `client_capabilities()` and `server_settings()`, so both transports now negotiate the same
  handshake.
- `LspClient::handshake()` exposes the `initialize` result, and the position-encoding refusal moved
  into a shared `refuse_foreign_encoding` that the bridge path now runs too.
- `LspClient::set_request_timeout`, driven from `--indexing-budget` (default 600s, matching the
  backend's own warm-up budget).
- `map_lsp_error` classifies by whether waiting can fix it: `ContentModified` and `Timeout` →
  `ServerCatchingUp`; every other server error keeps its code and message.

## And one change that is about legibility, not correctness

An expired budget now reports `IndexingIncomplete` when the server never answered, and an absent
assist only when it did — and the absent-assist message **names the titles the server offered**:

```
rust-analyzer offers no "extract into function" assist for the given range (it offered none)
```

That parenthesis is what identified D4. Before it, a wrong assist title, an unrefactorable range,
and a server that had been told nothing about its client all produced byte-identical output. The
diagnostic that would have found this in minutes did not exist, which is the more useful lesson than
any of the four defects: **the four hours went to the absence of the diagnostic, not to the bugs.**

## Evidence

Three scratch crates, run before and after. Before: `extract_method` refused on an inherent impl, on
a trait impl, and `extract_trait` refused too — which is what showed the range was not the subject.
After, all three succeed, and the trait-impl case settled a question the restructure plan had been
built around:

```rust
impl Svc for Impl {
    fn one(&self, n: u32) -> u32 {
        let mut acc = 0;
        self.sum_up(n, &mut acc);   // delegation written by rust-analyzer
        acc
    }
}

impl Impl {                          // block created by rust-analyzer
    fn sum_up(&self, n: u32, acc: &mut u32) { … }
}
```

Extracting out of a trait impl is *not* `E0407` and is not declined: rust-analyzer writes a new
inherent block and composes the delegating call itself. The `connection_service.rs` split plan had
90 hand-written delegations budgeted on the opposite belief; they are removed.

**Tests:** 8 added. `tddy-lsp` 26/26 (including an acceptance test that the launch spec's
capabilities reach the server — the assertion whose absence allowed D4), `tddy-code-restructuring`
218/218, `cargo clippy -- -D warnings` clean across every `tddy-lsp` consumer.

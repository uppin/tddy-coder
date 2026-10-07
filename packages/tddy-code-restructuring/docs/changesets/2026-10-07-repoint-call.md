# 2026-10-07 — `repoint_call` re-points a call's callee or every receiver of a method

**Type:** Feature

A new Rust operation, `repoint_call`, rewrites the part of a call that is in front of its argument
list and keeps the arguments byte for byte. The **single form** replaces one call's callee
(`self.slot(x)` → `self.peer.slot(x)`); the **bulk form**, anchored on a method, inserts hops after
the receiver of every call of that method the server knows (`x.m(..)` → `x.agent_roster().m(..)`).
`restructure verify` is taught to account for a declared re-point. Behaviour:
[repoint-call.md](../repoint-call.md).

## What changed

- **Plan surface** — `RefactorKind::RepointCall` (`plan/refactor_kind.rs`); `RefactorOp.callee:
  Option<String>` (`plan.rs`), with `callee: None` on the full `RefactorOp` literals; the refusals in
  `plan/codec/repoint_call_fields.rs` (new), wired through `plan/codec.rs`; `SUPPORTED` 23 → 24
  (`backends/rust.rs`). The callee parsers `rust_syntax::one_callee` and `one_receiver_template`.
- **The single form** — `backends/rust/repoint_call/single.rs`: `rewrite_callee` replaces everything
  before the argument list's `(`, reusing `call_in` for the "exactly one call" reading; `findings`
  answers a range anchor from the text alone, before any server.
- **The bulk form** — `backends/rust/repoint_call/sites.rs` (the reference classification, the
  all-at-once refusal, the per-file insertions) and `receivers.rs` (the backwards receiver walk,
  `receiver_span` / `insertions_for`), over `RustBackend::sites_of`. `repoint_call.rs` dispatches on
  the lowered range and holds `findings`.
- **`verify`** — `verify/repoint.rs` (new) holds `Repoint` and rule **R-call** (declared call
  re-point pairing), run between the visibility pairing and the re-point pairing and counted into
  `Excused::repointed`. The declaration travels as `RestructureVerifyArgs.repoint` →
  `Options.repoints` → `runner::verify` → `VerifyRequest.repoints` (proto field 4), through
  `tddy-tools`'s `index_client.rs::verify` and the daemon's `cli.rs` / `queries.rs`.
- **Tests** — `tests/repoint_call_plan_acceptance.rs` (library), `tests/verify_accounts_for_declared_repoints.rs`
  (library) and `tests/repoint_call_acceptance.rs` (live rust-analyzer, `cargo check` as the
  assertion; registered in `.config/nextest.toml` and `.config/rust-e2e.filterset`); inline unit
  tests in `repoint_call/{single,receivers}.rs` and `verify/repoint.rs`; `tddy-index-daemon`
  `tests/dual_transport_acceptance.rs` pins the declaration through the CLI and the daemon.

## Code issues

| Record | Measurement |
|---|---|
| `oversized-file-backends-rust.md` | **Grown, not closed**: wiring only (the `RepointCall` `SUPPORTED` entry, one `check` arm, one `resolve` arm — ~11 lines); 2,986 → 2,997 production lines; all logic in `backends/rust/repoint_call/`. History row appended; record kept. Split deferred — the sibling `#sharpen` nodes still touch this file |
| `dead-code-plan-filehint-modified.md` | **Unchanged**: this PR added `callee` to `RefactorOp` in `plan.rs` but not near `FileHint::modified`; still one write site, no reader. Row appended; record kept |

Cross-package entry: [docs/dev/changesets](../../../../docs/dev/changesets/2026-10-07-repoint-call.md).

# 2026-10-21 — `detach_method`
**Type:** Feature

`RefactorKind::DetachMethod` with its codec rules (`plan/codec/detach_fields.rs`), plain-`check` findings and a
live binding in `backends/rust/detach_method/` (`survey`, `block`, `member`, `refusals`, `calls`, `scan`,
`splice`, `build`, `server`), behind a `DetachServer` seam so the library suite runs against recorded server
answers (`tests/detach_method.rs`, 17 tests) and only two tests start rust-analyzer
(`tests/detach_method_acceptance.rs`, registered in `.config/rust-e2e.filterset` and `nextest.toml`).
`backends/rust.rs` gains `pub mod detach_method;` and one `SUPPORTED` entry (27 to 28); the kind is routed
through `same_crate_dispatch.rs`. `retarget_impl::outline::Operation` gains `DetachMethod`, which words the
two-blocks refusal for a detach and adds no trait-impl refusal (the operation's own R5 names the member).

`verify`: `verify/detach.rs` (`Detach`, R-detach `account`), a pass between rebind and repoint in
`compare_with`; `Declared.detaches` and `Declared::with_detaches` (implies the rebind), `--detach TYPE=NAME`
on `restructure verify`, and `runner::Options.detaches`.

Docs: [detach-method.md](../detach-method.md),
[rust-code-restructuring.md § `detach_method`](../../../../docs/ft/coder/rust-code-restructuring.md#detach_method).

# 2026-10-21 — the extract-into-function assist writes a stray `*` before `format!(...)` (E0277)

**Category:** Future enhancement (engine gap, rust-analyzer assist)
**Source:** `#host-desktops` 10/10, [#693](https://github.com/uppin/tddy-coder/pull/693) — plan `16-host-targets-start-host-stream`

## What happened

Plan 16 extracts `find_host_target` out of `start_host_stream_impl`
(`packages/tddy-screen-sharing/src/screen_sharing_service/host_targets.rs`). `restructure check --deep` said
`no findings`; the apply then failed its compile gate. In the new function the assist wrote
`Status::not_found(*format!(` (line 322) where the original read `Status::not_found(format!(`:

```
host_targets.rs:322:31: error[E0277]: the trait bound `str: Into<std::string::String>` is not satisfied
host_targets.rs:322:31: error[E0277]: the size for values of type `str` cannot be known at compilation time
```

The stray `*` comes from rust-analyzer's "extract into function" assist around a `format!` argument inside an
`ok_or_else` closure; the engine passes the assist's text through.

## Workaround used

The developer pre-approved one-token post-move build fixes (2026-10-21): the `*` was deleted by hand, then
`apply --resume` applied nothing (`applied 0 of 2`), so `cargo fix --lib --tests -p tddy-screen-sharing
--allow-dirty` (one fix: the now-unused `mut` on `prepared`) and `cargo fmt -p tddy-screen-sharing` finished the
plan.

## Fix

`check --deep` should compile the extracted function, or the engine should refuse an extraction whose assist
output differs from the selected text apart from the moved lines.

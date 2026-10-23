# WASI preview 2 subset (hand-written)

The interfaces of `wasi:{io,cli,clocks,filesystem}@0.2.6` that a `wasm32-wasip2` Zed extension
imports — read off the fixture component's import types (`tests/fixtures/wasm-ext`) — and only the
functions and types it imports. The names, order and shapes are the WASI 0.2.6 WIT's, so a
component importing them type-checks against `src/wasm/wasi.rs`.

Written by hand because `wasmtime-wasi` (the crate that ships the full interfaces and their host
implementation) is a new dependency nobody has consented to. If it is consented, this directory
and `src/wasm/wasi.rs` are replaced by `wasmtime_wasi::p2::add_to_linker_sync`.

A component importing a WASI function this subset does not list does not load: its `Load` error
names the import.

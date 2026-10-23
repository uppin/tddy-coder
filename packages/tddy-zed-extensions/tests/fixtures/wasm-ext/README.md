# wasm-fixture (test fixture code extension)

A tiny Zed code extension, laid out as Zed lays out a dev extension (`extension.toml`, `Cargo.toml`,
`src/`) plus the compiled `extension.wasm` an install would put beside its manifest. The
`extension-wasm-command` suites run it through `tddy_zed_extensions::wasm`; nothing builds it during
`cargo build` or `cargo test` — it is its own Cargo workspace, outside tddy's.

`src/lib.rs` answers `language_server_command` per server id: `fake-ls` → whatever
`worktree.which("fake-ls")` finds; `downloading-ls` → calls `download_file`; `looping-ls` → never
returns.

| | |
|---|---|
| API | `zed_extension_api` 0.6.0 (WIT `since_v0.6.0`), `Cargo.lock` committed |
| Target | `wasm32-wasip2` (a WASI preview 2 component) |
| Toolchain | rustc 1.94.1 (`e408947bf 2026-03-25`) — the dev shell's, plus the `wasm32-wasip2` std from the flake's pinned `rust-overlay` |
| Size | 154,907 bytes |
| sha256 | `4971530753aebbfcc5d16adff7644b3791a984cb1113905d544e3936e0e26720` (two clean builds, identical) |

Rebuild (from this directory; the toolchain is the dev shell's rustc with the extra target):

```bash
TC=$(nix build --no-link --print-out-paths --impure --expr '
  let f = builtins.getFlake (toString ../../../../..);
      pkgs = import f.inputs.nixpkgs { system = builtins.currentSystem;
                                       overlays = [ f.inputs.rust-overlay.overlays.default ]; };
  in pkgs.rust-bin.stable."1.94.1".default.override { targets = [ "wasm32-wasip2" ]; }')
PATH="$TC/bin:$PATH" CARGO_TARGET_DIR="$(mktemp -d)" sh -c \
  'cargo build --release --locked --target wasm32-wasip2 &&
   cp "$CARGO_TARGET_DIR/wasm32-wasip2/release/tddy_fixture_wasm_ext.wasm" extension.wasm'
```

Change the source only together with the `.wasm`, and update the size and sha256 above.

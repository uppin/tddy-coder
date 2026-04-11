{
  description = "tddy-coder development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ rust-overlay.overlays.default ];
        };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rustfmt" "clippy" ];
        };
      in
      {
        devShells.default = pkgs.mkShell {
          nativeBuildInputs = [
            pkgs.pkg-config
          ];
          buildInputs = [
            pkgs.glib
            pkgs.fontconfig
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
            pkgs.libva
            # tddy-livekit-screen-capture → xcap → wayland-sys / gbm (pkg-config)
            pkgs.wayland
            pkgs.wayland-protocols
            pkgs.libdrm
            pkgs.mesa
            # khronos-egl (screen capture stack) needs egl.pc
            pkgs.libglvnd
            pkgs.libgbm
            pkgs.libxcb
            pkgs.pipewire
            # libspa-sys (pipewire) uses bindgen — use Nix libclang, not host /usr/lib/llvm-*
            pkgs.llvmPackages.libclang
            pkgs.stdenv.cc.cc.lib
          ];
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            pkgs.buf
            pkgs.protobuf
            pkgs.bzip2
            pkgs.git
            pkgs.bun
            pkgs.nodejs_20
            pkgs.util-linux
          ];
          shellHook = ''
            echo "tddy-coder dev shell: rustc, cargo, rustfmt, clippy, rust-analyzer, bun, node"
          '' + pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
            export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          '' + ''
            if _tddy_root="$(git rev-parse --show-toplevel 2>/dev/null)"; then
              if [[ -d "$_tddy_root/node_modules/.bin" ]]; then
                export PATH="$_tddy_root/node_modules/.bin:$PATH"
              fi
            fi
          '' + pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isDarwin ''
            export CXXFLAGS="-include ''${SDKROOT}/usr/include/uuid/uuid.h''${CXXFLAGS:+ $CXXFLAGS}"
          '';
        };
      }
    );
}

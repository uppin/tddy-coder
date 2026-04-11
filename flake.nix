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
        devShells.default = pkgs.mkShell ({
          nativeBuildInputs = [
            pkgs.pkg-config
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
            # libspa-sys uses bindgen; point at Nix libclang (avoid broken /usr/lib/llvm-* in pure shells)
            pkgs.clang
          ];
          buildInputs = [
            pkgs.glib
            pkgs.fontconfig
          ] ++ pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [
            # libstdc++.so.6 for prebuilt Node native addons and bindgen/libclang if host LLVM is probed.
            pkgs.stdenv.cc.cc.lib
            pkgs.libva
            # tddy-livekit-screen-capture → xcap → wayland-sys / gbm / drm (pkg-config)
            pkgs.wayland
            pkgs.wayland-protocols
            pkgs.libdrm
            pkgs.pipewire
            # khronos-egl (gbm / GPU capture path) needs egl.pc (libglvnd.dev on current nixpkgs)
            pkgs.libglvnd.dev
            # xcap → gbm-sys / X11 path: link needs libgbm and libxcb (-lgbm -lxcb)
            pkgs.libgbm
            pkgs.libxcb
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
        } // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
        });
      }
    );
}

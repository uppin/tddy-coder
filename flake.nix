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

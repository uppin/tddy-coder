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
          extensions = [ "rustfmt" "clippy" "llvm-tools-preview" ];
        };
        # Buildroot 2024.02.6 (LTS) — not in nixpkgs; fetched from buildroot.org.
        # BUILDROOT_DIR is exported in shellHook so the VM image build daemon can find it.
        buildrootSrc = pkgs.fetchzip {
          name = "buildroot-2024.02.6-src";
          url = "https://buildroot.org/downloads/buildroot-2024.02.6.tar.gz";
          sha256 = "1hkr8vh670wiiw97ikh7damb1qymbzhha4cdgy1idzf86v1vqf3y";
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
            # Tauri (packages/tddy-desktop/src-tauri) → wry/tao on Linux: WebKitGTK 4.1 plus the
            # GTK stack it links against. macOS needs none of this — the webview is the system
            # WKWebView. Without these, `cargo build --workspace` fails on Linux at pkg-config.
            pkgs.webkitgtk_4_1
            pkgs.libsoup_3
            pkgs.gtk3
            # TLS for the webview's own network stack (glib-networking provides the GIO TLS backend)
            pkgs.glib-networking
            # Tauri renders the tray/window icons through librsvg
            pkgs.librsvg
            pkgs.cairo
            pkgs.pango
            pkgs.gdk-pixbuf
            pkgs.atk
          ];
          packages = [
            rustToolchain
            pkgs.rust-analyzer
            # Test runner used by CI (.github/workflows/ci.yml) and available
            # locally so a CI failure can be reproduced with the same command.
            pkgs.cargo-nextest
            pkgs.buf
            pkgs.protobuf
            # `cargo tauri dev` / `cargo tauri build` for packages/tddy-desktop.
            pkgs.cargo-tauri
            pkgs.bzip2
            pkgs.git
            pkgs.bun
            pkgs.nodejs_20
            pkgs.util-linux
            pkgs.gnumake
            pkgs.qemu
            pkgs.xorriso
            pkgs.openssh
          ];
          shellHook = ''
            echo "tddy-coder dev shell: rustc, cargo, rustfmt, clippy, rust-analyzer, bun, node"
            export BUILDROOT_DIR="${buildrootSrc}"
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

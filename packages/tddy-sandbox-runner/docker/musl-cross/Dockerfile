# Cross-compile tddy-sandbox-runner for x86_64-unknown-linux-musl (the QEMU guest's
# architecture, regardless of the host's), reusing the pattern established for the
# Buildroot host toolchain (packages/tddy-vm/docker/buildroot-host/Dockerfile): a real
# Linux musl toolchain via Docker, since the local Nix-provided rustc has no musl target
# and there's no rustup here to add one. Alpine's rustc/cargo already target musl
# natively (the whole distro uses musl libc), so no `--target` flag is needed.
FROM --platform=linux/amd64 rust:1-alpine

RUN apk add --no-cache musl-dev build-base pkgconfig protobuf protobuf-dev

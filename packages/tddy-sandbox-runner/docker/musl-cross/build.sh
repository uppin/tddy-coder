#!/usr/bin/env bash
# Cross-compile tddy-sandbox-runner for x86_64-unknown-linux-musl (the QEMU guest's
# architecture) using a Docker toolchain, since the local Nix-provided rustc has no musl
# target and there's no rustup to add one (see the Dockerfile in this directory).
#
# Output: $OUTPUT_DIR/tddy-sandbox-runner (default: ~/.cache/tddy-vm-build/tddy-sandbox-runner-musl),
# the directory tddy-sandbox-qemu's --runner-dir flag expects (9p-mounted as a whole dir
# into the guest).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
IMAGE_TAG="tddy-sandbox-runner-musl-cross"
OUTPUT_DIR="${OUTPUT_DIR:-$HOME/.cache/tddy-vm-build/tddy-sandbox-runner-musl}"

echo "Building musl cross-compile toolchain image..."
docker build -t "$IMAGE_TAG" -f "$REPO_ROOT/packages/tddy-sandbox-runner/docker/musl-cross/Dockerfile" \
    "$REPO_ROOT/packages/tddy-sandbox-runner/docker/musl-cross"

echo "Cross-compiling tddy-sandbox-runner (release, x86_64-unknown-linux-musl)..."
# The repo is mounted read-write (not :ro): tddy-workflow-recipes' build.rs writes
# generated code back into the source tree as part of a normal build, which a read-only
# mount would break. This is our own repo, not a foreign source tree (contrast with the
# Buildroot mirror, which protects a Nix store path we don't own) — verify with `git
# status` afterward if you want to confirm nothing unexpected changed.
docker run --rm \
    -v "$REPO_ROOT:/repo" \
    -v tddy-sandbox-runner-musl-target:/target \
    -v tddy-sandbox-runner-musl-cargo-home:/cargo-home \
    -w /repo \
    -e CARGO_TARGET_DIR=/target \
    -e CARGO_HOME=/cargo-home \
    "$IMAGE_TAG" \
    cargo build --release -p tddy-sandbox-runner

mkdir -p "$OUTPUT_DIR"
echo "Extracting binary to $OUTPUT_DIR ..."
docker run --rm \
    -v tddy-sandbox-runner-musl-target:/target \
    -v "$OUTPUT_DIR:/host-out" \
    "$IMAGE_TAG" \
    cp /target/release/tddy-sandbox-runner /host-out/

echo "Done: $OUTPUT_DIR/tddy-sandbox-runner"

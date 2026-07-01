# Linux host toolchain for building Buildroot images, used on macOS where Buildroot's
# dependency checker (support/dependencies/dependencies.sh) rejects Apple Clang's `gcc`
# trampoline and expects several Linux-only tools. See run_buildroot_pipeline in
# packages/tddy-vm/src/build.rs.
FROM debian:12-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    bc \
    build-essential \
    ca-certificates \
    cpio \
    file \
    git \
    patch \
    perl \
    python3 \
    rsync \
    unzip \
    wget \
    && rm -rf /var/lib/apt/lists/*

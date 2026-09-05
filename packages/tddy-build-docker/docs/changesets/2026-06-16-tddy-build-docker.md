# 2026-06-16 — **tddy-build-docker

**Type:** Feature

new plugin crate** — extracted from `tddy-build` plugin architecture refactor; lowers `docker_image` targets to `docker build -f <dockerfile> -t <tag> [--build-arg …] <context>` with `--iidfile` for output tracking; `deny_unknown_fields` config struct. Feature: [docs/ft/build/tddy-build.md](../../../../docs/ft/build/tddy-build.md). (tddy-build-docker)

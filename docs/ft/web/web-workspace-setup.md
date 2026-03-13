# Web Workspace Setup

## Summary

Set up a Bun-based monorepo workspace (yarn-style `workspaces` field) at the repo root, coexisting with the existing Rust/Cargo workspace. The first web package is `tddy-web` — a React (with React Compiler) dashboard for dev progress tracking. The workspace includes Storybook 9 for component development and Cypress for both component and e2e testing.

## Background

The tddy-coder project is currently a pure Rust workspace. A web frontend is needed for a dev progress tracking dashboard (`tddy-web`). The workspace structure must support adding more web packages in the future alongside the existing Rust packages.

A web terminal feature already exists (see [web-terminal.md](web-terminal.md)) using ghostty-web; `tddy-web` is a separate dashboard application.

## Requirements

### Workspace Structure
- Root `package.json` with `workspaces` field pointing to web packages
- Bun as the package manager and bundler
- Web packages live alongside Rust packages in `packages/`
- Must not interfere with existing Cargo workspace

### tddy-web Package
- React with React Compiler
- Bun's native bundler for builds
- TypeScript
- Dashboard for dev progress tracking (initial scaffold)

### Storybook 9
- Storybook 9 configured for the workspace
- React/Vite framework integration
- Single example: a Button component with a story

### Cypress Testing
- **Component testing**: Cypress component test for the Button component
- **E2e testing**: Cypress e2e test that visits a Storybook story
- Single test for each type to validate the infrastructure

## Success Criteria

1. `bun install` from repo root installs all web dependencies
2. `tddy-web` package builds with Bun's native bundler
3. Storybook 9 launches and renders the Button story
4. Cypress component test passes for the Button component
5. Cypress e2e test visits Storybook and passes
6. Existing Rust `cargo build` and `cargo test` are unaffected

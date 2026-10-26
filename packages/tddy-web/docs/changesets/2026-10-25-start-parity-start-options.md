# 2026-10-25 — Stack-picker parity fixture

**Type:** Test

`#start-parity` 5/9 ([#754](https://github.com/uppin/tddy-coder/pull/754)).
`cypress/component/stackParentsParity.cy.ts` runs `prStackOrchestrators` and
`stackBaseSessionCandidates` over `cypress/support/fixtures/stack-parent-candidates.json`, the fixture
`tddy-tool-engine/tests/start_options.rs` runs the daemon's Rust mirror over.

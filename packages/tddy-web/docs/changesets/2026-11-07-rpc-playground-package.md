# 2026-11-07 — RPC playground screen moved to `tddy-rpc-playground-web` (#web-split 9/17, PR #846)

`src/rpc-playground/` keeps only `RpcPlaygroundAppPage` (shell, sign-in, presence, reflection over
the selected host), which imports the screen and helpers from `tddy-rpc-playground-web`. The shell's
RPC Playground menu-entry tests stay here as `cypress/component/DaemonNavMenuRpcPlayground.cy.tsx`.
The fake transports and the unused `anEchoRegistry` builder left `src/test-utils/builders.ts`.

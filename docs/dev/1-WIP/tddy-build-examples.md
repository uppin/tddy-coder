# tddy-build example projects, logging & plugin inputs/outputs

## tddy-build
- Engine now logs at discovery / lowering / cycle-detection / cache / execution
  seams (`log` crate). Cycle detection emits a `warn!` naming the offending target.
- New `io` helper (`OutputSpec`, `srcs_to_inputs`, `outputs_to_decls`) lets recipe
  plugins declare cacheable inputs/outputs in open config.
- Executor now creates declared output parent directories before running an action
  (tools like `docker build --iidfile` and most compilers don't create them).
- Added a runnable `script`/`tool`/`group` example under `examples/pipeline/`.

## tddy-build-rust / -typescript / -docker
- Recipe plugins now emit `inputs`/`outputs` on their lowered actions
  (rust: `srcs`+`outputs`+`working_dir`; typescript: `srcs`+`output_dirs`;
  docker: `srcs`+`outputs` with `--iidfile`), so the content-addressed cache
  invalidates on source edits.
- Each plugin ships a real, interdependent multi-package example project
  (`examples/workspace`, `examples/monorepo`, `examples/images`) with integration
  tests covering deps-first ordering, real builds (cargo/bun/docker, tool-gated),
  cache hit/miss, and circular-reference detection. The rust example is excluded
  from the root cargo workspace.

## Note for architecture.md
Update `packages/tddy-build/docs/architecture.md` "Pipeline" + "Consumers"
sections to mention engine logging, the plugin-declared inputs/outputs, and output
parent-dir creation once this changeset lands (handled via the normal changeset →
docs merge).

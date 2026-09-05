# 2026-03-10 — Session ID at Startup, SIGINT Graceful Shutdown

**Type:** Feature

`run_main(args)`: panic hook, ctrlc handler (graceful shutdown, no exit in handler), run_with_args, exit(130) after return. Args.session_id (UUID v7 at startup). `session_dir_path(args)`, `print_session_id_on_exit(session_id, path)`. Session info printed on exit (TUI and plain). SIGINT integration test: tddy_demo_sigint_prints_session_info_to_stderr. (tddy-coder)

# 2026-03-18 — Debug, Demo Worktree, Workflow Logging

**Type:** Feature

init_tddy_logger(debug, debug_output_path, webrtc_debug_output_path). WebRTC detection via `.cc:` or `.cpp:`; libwebrtc logs route to separate file when set. ensure_worktree_for_acceptance_tests skips worktree when backend_name == "stub" (demo); uses output_dir directly. log::error! on worktree creation failure (repo_root, plan_dir). Workflow failure log::info! → log::error! for debug visibility. (tddy-core)

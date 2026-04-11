//! tddy-coder CLI binary.

use clap::Parser;
use tddy_coder::{load_config, merge_config_into_args, run_main, Args, CoderArgs};

/// When Codex runs `BROWSER <url>` with `TDDY_CODEX_OAUTH_OUT` set (during `codex exec` or
/// `codex login`), write the authorize URL and exit without starting the full CLI (see
/// `CodexBackend::spawn_oauth_login`, LiveKit participant metadata).
fn codex_oauth_browser_hook_early_exit() {
    let args: Vec<String> = std::env::args().collect();
    if std::env::var_os("TDDY_CODEX_OAUTH_OUT").is_none() {
        return;
    }
    let Some(url) = args
        .iter()
        .skip(1)
        .find(|a| a.starts_with("https://"))
        .map(String::as_str)
    else {
        return;
    };
    let Ok(out) = std::env::var("TDDY_CODEX_OAUTH_OUT") else {
        return;
    };
    if out.trim().is_empty() {
        return;
    }
    if let Err(e) = std::fs::write(&out, url) {
        eprintln!(
            "tddy-coder: codex oauth hook could not write {}: {}",
            out, e
        );
        std::process::exit(1);
    }
    std::process::exit(0);
}

fn main() {
    codex_oauth_browser_hook_early_exit();
    let coder_args = CoderArgs::parse();
    let config_path = coder_args.config.clone();
    let mut args: Args = coder_args.into();

    if let Some(ref path) = config_path {
        match load_config(path) {
            Ok(config) => merge_config_into_args(&mut args, config),
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }

    run_main(args);
}

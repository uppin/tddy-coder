//! tddy-demo — Same app as tddy-coder, with StubBackend.
//!
//! Identical TUI, CLI, and workflow. Only the backend differs (StubBackend).
//! Uses tddy-coder lib with DemoArgs (agent: stub only).

use std::env;

use clap::Parser;
use tddy_coder::{load_config, merge_config_into_args, run_main, Args, DemoArgs};

fn main() {
    if env::var("TDDY_SESSIONS_DIR").is_err() {
        env::set_var("TDDY_SESSIONS_DIR", env::temp_dir().join("tddy-demo"));
    }

    let mut cli_args: Vec<String> = env::args().collect();
    let has_agent = cli_args
        .iter()
        .skip(1)
        .position(|a| a == "--agent" || a == "-a");
    if has_agent.is_none() {
        cli_args.insert(1, "stub".to_string());
        cli_args.insert(1, "--agent".to_string());
    }
    let demo_args = DemoArgs::parse_from(cli_args);
    let config_path = demo_args.config.clone();
    let mut args: Args = demo_args.into();

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

//! tddy-demo — Same app as tddy-coder, with StubBackend.
//!
//! Identical TUI, CLI, and workflow. Only the backend differs (StubBackend).
//! Uses tddy-coder lib with DemoArgs (agent: stub only).

use std::env;

use clap::Parser;
use tddy_coder::{run_main, Args, DemoArgs};
use tddy_core::init_tddy_logger;

fn main() {
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
    let args: Args = demo_args.into();
    init_tddy_logger(args.debug, args.debug_output.as_deref());
    run_main(args);
}

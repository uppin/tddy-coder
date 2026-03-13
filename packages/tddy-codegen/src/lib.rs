//! Build-time codegen for transport-agnostic RPC service traits and adapters.

mod config;
mod generator;

pub use config::TddyServiceGenerator;
pub use generator::LiveKitServiceGenerator;

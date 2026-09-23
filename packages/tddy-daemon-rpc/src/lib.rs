//! The daemon's RPC-family handlers, one struct per family.
//!
//! Each family's wire protocol was already its own service — `project.proto`, `catalog.proto`,
//! `exec_tools.proto`, `pr_stack.proto` — served through a `*ServiceImpl<H>` generic over its
//! handler. What was not decomposed was the handler: every family was an `impl` on the one
//! `DaemonSessionHost`, reading any of its 31 fields. Here each family is a struct holding only the
//! fields it uses, built from the host by `from_host` and sharing the host's state through the same
//! `Arc`s rather than copies of it.
//!
//! No handler holds the host. The lifecycle crate reaches these handlers only through its own
//! `DaemonRpcFamilies` port, which the composition root fills.

pub mod catalog;
pub mod exec_tool;
pub mod families;
pub mod pr_stack;
pub mod project;
pub mod test_util;

pub use catalog::CatalogRpcHandler;
pub use exec_tool::ExecToolRpcHandler;
pub use families::RpcHandlers;
pub use pr_stack::PrStackRpcHandler;
pub use project::ProjectRpcHandler;

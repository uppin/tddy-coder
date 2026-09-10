//! Discovery agent — OpenAI-compatible multi-turn codebase exploration, the specialized
//! subagents built on it, and the session-scoped runtime that keeps track of both.

pub mod agent_def;
pub mod backend;
pub mod discovery;
pub mod openai;
/// The live roster of agents attached to a session: what the daemon says is attached, what those
/// agents have taken over, and the conversation RPCs that reach the ones this process cannot run.
pub mod roster;
pub mod subagent;
/// Every conversation a session has open with a subagent, and what each has spent.
pub mod subagent_runtime;
pub mod tools;
pub mod warmup;

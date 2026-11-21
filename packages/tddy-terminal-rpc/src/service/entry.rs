use std::sync::Arc;

use crate::proto::terminal_session::TerminalSessionServiceServer;
use crate::service::{TerminalSessionPorts, TerminalSessionServiceImpl};

/// The coordinate `terminal_session.proto`'s nine methods are served at.
///
/// A routing address has to be spelled the same by the host that registers it and by every caller
/// that puts it on the wire, and nothing checks that it is: a mismatch is a **runtime** "unknown
/// service", reported as a failure of the method rather than of the spelling. So the name is
/// published once, here, and consumed by both ends.
///
/// It lives in this crate rather than in `tddy-service` (where the cross-crate coordinates of
/// protos `tddy-service` owns are published) because this crate owns `terminal_session.proto`, is
/// the only crate that *serves* it — [`build_terminal_session_entry`], and the coder's wrapper
/// around it, which reads the name off that entry — and is already a dependency of every caller
/// that addresses it, [`crate::pty_relay`] included.
pub const TERMINAL_SESSION_SERVICE: &str = "terminal_session.TerminalSessionService";

/// The `terminal_session.TerminalSessionService` entry a host's wiring layer registers.
///
/// `#unbundle` node 6 made this crate *serve* the proto it already owned: the nine methods had been
/// extracted here as [`crate::bridge`] functions but were only ever reachable through
/// `the pre-unbundle monolithic RPC coordinate`'s duplicate copy of the same schema, with hand-written converters
/// between the two. Both the copy and the converters are gone; this entry is where the nine
/// answer.
///
/// Takes the whole [`TerminalSessionPorts`] because every field is wiring rather than behaviour:
/// which identity a token belongs to, which OS user it maps to, where the live terminals and the
/// control lease live, and what the host's transport accepts as one frame.
#[must_use]
pub fn build_terminal_session_entry(ports: TerminalSessionPorts) -> tddy_rpc::ServiceEntry {
    let server = TerminalSessionServiceServer::new(TerminalSessionServiceImpl::new(ports));
    tddy_rpc::ServiceEntry {
        name: TERMINAL_SESSION_SERVICE,
        service: Arc::new(server) as Arc<dyn tddy_rpc::RpcService>,
    }
}

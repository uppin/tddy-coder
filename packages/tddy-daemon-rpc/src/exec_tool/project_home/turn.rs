//! `SendSessionMessage` and `ResumeSession` answered on this host (`#orchestrators` 4/11): a turn is
//! delivered by the `turn_input` the target's agent spec declares, without changing who holds its
//! terminal's control lease, and a resume is the web's Resume plus an optional first message.

use crate::exec_tool::project_home::DaemonProjectHome;
use tddy_tool_engine::project_home_turn_tools::ProjectHomeTurnPort;

// TODO(session-turn-control): implement — `send_session_message` (terminal paste + submit key, ACP
// prompt; refusals for no `turn_input`, a stopped session and a pending question or permission) and
// `resume_session` (`resume_session_at_session_coordinate`, then the first message once the agent's
// terminal is ready). Until then the trait's defaults refuse.
impl ProjectHomeTurnPort for DaemonProjectHome {}

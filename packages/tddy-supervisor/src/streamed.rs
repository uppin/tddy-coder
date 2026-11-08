//! `SpawnStreamed`: one binary as another OS user, its stderr streamed line by line.
//!
//! The generic successor of `CloneRepository`: the supervisor runs what the daemon asked for, under
//! the same user, exec and environment gates as `SpawnSession`, and reports `Started{pid}`, one
//! `Line` per line of stderr (split on `\r` as well as `\n`), then `Exited{code, signal,
//! stderr_tail}`. It knows nothing about what the binary is: a clone's progress is parsed, and its
//! partial directory moved into place, by the daemon.

use futures_util::Stream;
use tddy_rpc::Status;

use crate::proto::supervisor::StreamedEvent;

/// A streamed spawn's events as the generated server's stream type.
pub(crate) type StreamedEvents =
    std::pin::Pin<Box<dyn Stream<Item = Result<StreamedEvent, Status>> + Send>>;

//! The relay loop behind a forwarded server stream.
//!
//! Split out of [`crate::peer_forwarding`] so the idle policy can be exercised without a LiveKit
//! room: the clock is the only thing the policy depends on.

use std::time::Duration;

use tddy_rpc::Status;
use tokio::sync::mpsc::{Receiver, UnboundedSender};

/// How a relayed stream ended in error: the peer said so, or the relay gave up on a silent peer.
///
/// The two are produced by different branches of [`Relay::run`] and kept apart as variants, so a
/// consumer never has to guess from a status code: a peer may send `DEADLINE_EXCEEDED` of its own
/// (that is an answer), and the relay's idle expiry is the only thing that means silence.
#[derive(Debug)]
pub(crate) enum RelayFailure {
    /// A status the peer sent: a terminal error frame, or a frame this side could not decode.
    Peer(Status),
    /// The relay's idle timeout expired with no frame from the peer.
    Silent(Status),
}

impl RelayFailure {
    /// The status to hand a consumer that does not distinguish the two.
    pub(crate) fn into_status(self) -> Status {
        match self {
            RelayFailure::Peer(status) | RelayFailure::Silent(status) => status,
        }
    }
}

/// How a forwarded stream is carried: how long it may go quiet, how a frame is read, and what a quiet
/// spell becomes.
pub(crate) struct Relay<D, I> {
    /// Per frame: every frame, whatever it holds, restarts it.
    pub idle_timeout: Duration,
    pub decode_frame: D,
    /// The status reported for a silent peer, carried as [`RelayFailure::Silent`].
    pub idle_status: I,
}

impl<D, I> Relay<D, I>
where
    I: Fn() -> Status,
{
    /// Carry `frames` to `tx`, decoding each, until the peer ends the stream. Each terminal error is
    /// passed through `classify`, which picks the consumer's error type.
    ///
    /// A peer that keeps sending (a start emitting keep-alives while it waits on a clone) is never cut
    /// off, while one that goes silent ends the relayed stream with an error — never a short success,
    /// because a consumer cannot tell a truncated stream from a whole one.
    pub(crate) async fn run<T, E>(
        self,
        mut frames: Receiver<Result<Vec<u8>, Status>>,
        tx: UnboundedSender<Result<T, E>>,
        classify: impl Fn(RelayFailure) -> E,
    ) where
        D: Fn(Vec<u8>) -> Result<T, Status>,
    {
        loop {
            let item = match tokio::time::timeout(self.idle_timeout, frames.recv()).await {
                Ok(Some(Ok(bytes))) => (self.decode_frame)(bytes).map_err(RelayFailure::Peer),
                // A mid-stream failure arrives as a single terminal error frame.
                Ok(Some(Err(status))) => Err(RelayFailure::Peer(status)),
                // The peer sent end-of-stream: the relayed stream ends the same way.
                Ok(None) => break,
                Err(_) => Err(RelayFailure::Silent((self.idle_status)())),
            };
            let item = item.map_err(&classify);
            let terminal = item.is_err();
            if tx.send(item).is_err() || terminal {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tddy_rpc::{Code, Status};
    use tokio::sync::mpsc::{channel, unbounded_channel};

    const IDLE: Duration = Duration::from_secs(30);

    fn timed_out() -> Status {
        Status::deadline_exceeded("idle")
    }

    /// Relay what `frames` carries with a 30 s idle timeout; returns what came out the far side.
    async fn relayed(
        frames: tokio::sync::mpsc::Receiver<Result<Vec<u8>, Status>>,
    ) -> Vec<Result<Vec<u8>, Status>> {
        let (tx, mut out) = unbounded_channel();
        Relay {
            idle_timeout: IDLE,
            decode_frame: Ok,
            idle_status: timed_out,
        }
        .run(frames, tx, RelayFailure::into_status)
        .await;
        std::iter::from_fn(|| out.try_recv().ok()).collect()
    }

    #[tokio::test(start_paused = true)]
    async fn frames_spaced_under_the_idle_timeout_never_time_the_stream_out() {
        // Given a peer that sends a frame every 20 s for 100 s, then ends the stream
        let (tx, frames) = channel(8);
        tokio::spawn(async move {
            for n in 0..5u8 {
                tokio::time::sleep(Duration::from_secs(20)).await;
                tx.send(Ok(vec![n])).await.expect("the relay is listening");
            }
        });

        // When the relay carries it
        let out = relayed(frames).await;

        // Then every frame arrived, and none of them is an error
        let frames: Vec<_> = out.into_iter().map(|f| f.expect("a frame")).collect();
        assert_eq!(frames, vec![vec![0], vec![1], vec![2], vec![3], vec![4]]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_silence_longer_than_the_idle_timeout_ends_the_stream_as_an_error() {
        // Given a peer that sends one frame and then goes quiet, without ending the stream
        let (tx, frames) = channel(8);
        tx.send(Ok(vec![1])).await.expect("buffered");

        // When the relay carries it
        let out = relayed(frames).await;
        drop(tx);

        // Then the frame arrives, then the stream ends with the timeout rather than just ending
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[1].as_ref().expect_err("a timeout").code,
            Code::DeadlineExceeded
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_frame_is_relayed_as_the_decoder_returns_it() {
        let (tx, frames) = channel(8);
        tx.send(Ok(b"keep-alive".to_vec())).await.expect("buffered");
        drop(tx);

        let out = relayed(frames).await;

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].as_ref().expect("a frame"), b"keep-alive");
    }
}

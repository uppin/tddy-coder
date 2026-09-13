//! Wire-identical protobuf messages split across packages share the same bytes on the wire.
//!
//! Node 8 moved families A, L and P to `catalog`, `exec_tools` and `pr_stack` while the daemon's
//! routing helpers were written against `connection` types. Bridging through encode/decode keeps
//! one implementation path until those helpers are retargeted at the served coordinates.

use prost::Message;
use tddy_rpc::Status;

/// Decode `src` as `Dst` when both messages share the same field layout on the wire.
pub(crate) fn wire_same<Src: Message, Dst: Message + Default>(src: &Src) -> Result<Dst, Status> {
    Dst::decode(src.encode_to_vec().as_slice())
        .map_err(|e| Status::internal(format!("family proto bridge decode: {e}")))
}

/// Same as [`wire_same`] for blocking closures that return `anyhow::Result`.
pub(crate) fn wire_same_anyhow<Src: Message, Dst: Message + Default>(
    src: &Src,
) -> anyhow::Result<Dst> {
    wire_same(src).map_err(|s| anyhow::anyhow!(s.to_string()))
}

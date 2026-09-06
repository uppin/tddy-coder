//! Bringing an image in from outside the library: what format the supplied file actually
//! is, the one-time convert that normalises a non-qcow2 one, and the refusal that keeps a
//! `qemu-img convert` from flattening a backing chain.
//!
//! Everything the library stores is qcow2 — `images/01-base/` because every layer above it
//! is created with `-F qcow2`, and each layer above because it is a delta of its parent. A
//! developer's supplied cloud image need not be: raw, VMDK and VDI are all common. Converting
//! such a source **once, on the way in** is format normalisation of an image that never had a
//! chain, and is the only convert this crate performs.
//!
//! Converting a **qcow2** source is a different operation wearing the same name: it reads the
//! whole backing chain and writes one standalone image, turning a cheap delta into a full copy
//! and severing its parent, with nothing on disk recording that it happened.
//! [`refuse_chain_flattening`] makes that argv unrepresentable rather than merely discouraged —
//! both of this crate's `qemu-img` runners (the async
//! [`crate::cloud_init::run_qemu_img`] and this module's blocking one) check it before
//! spawning anything.

use crate::vm::VmError;
use std::io::{BufReader, Read};
use std::path::Path;

/// The format name `qemu-img` uses for qcow2, and the only format the library stores.
pub const QCOW2_FORMAT: &str = "qcow2";

/// What a supplied image turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuppliedImageFormat {
    /// Already qcow2 — the library stores it as it is.
    Qcow2,
    /// Anything else, under the name `qemu-img` gives it (`raw`, `vmdk`, `vdi`, …), which is
    /// exactly what [`normalise_to_qcow2`] passes back as the input format.
    Other(String),
}

/// The virtual size `image` presents to a guest, in bytes.
///
/// The figure a *derived* image has to match or exceed. A qcow2 overlay may be created with
/// any size at all, including one smaller than the image it chains onto — `qemu-img` accepts
/// it without complaint, and the result is a guest whose partition table refers to sectors
/// the disk no longer has. Since a bake grows the root partition to fill whatever disk it
/// was given, that is not a corner case: it is what an overlay smaller than its parent
/// always produces, and it surfaces as `ALERT! PARTUUID=… does not exist` in an initramfs
/// shell rather than as an error from the tool that caused it.
pub fn virtual_size_bytes(image: &Path) -> Result<u64, VmError> {
    let info = qemu_img_info(image)?;
    info.get("virtual-size")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            VmError::BuildFailed(format!(
                "qemu-img info for {} reported no virtual-size",
                image.display()
            ))
        })
}

/// `qemu-img info --output=json`, parsed.
fn qemu_img_info(src: &Path) -> Result<serde_json::Value, VmError> {
    let output = std::process::Command::new("qemu-img")
        .args(["info", "--output=json"])
        .arg(src)
        .output()
        .map_err(|e| {
            VmError::BuildFailed(format!(
                "failed to spawn qemu-img info for {}: {e}",
                src.display()
            ))
        })?;
    if !output.status.success() {
        return Err(VmError::BuildFailed(format!(
            "qemu-img info {} failed: {}",
            src.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| {
        VmError::BuildFailed(format!(
            "failed to parse qemu-img info for {}: {e}",
            src.display()
        ))
    })
}

/// Detect the format of `src` by asking `qemu-img info`.
///
/// Unlike [`crate::library`]'s backing-file check, which reads the qcow2 header itself because
/// the question there is what the bytes say, this asks the tool that will later have to open
/// the file: the useful answer is the format `qemu-img` will act on, probing included.
pub fn supplied_image_format(src: &Path) -> Result<SuppliedImageFormat, VmError> {
    let info = qemu_img_info(src)?;
    let format = info
        .get("format")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            VmError::BuildFailed(format!(
                "qemu-img info for {} reported no format",
                src.display()
            ))
        })?;

    Ok(match format {
        QCOW2_FORMAT => SuppliedImageFormat::Qcow2,
        other => SuppliedImageFormat::Other(other.to_string()),
    })
}

/// Convert `src`, which is in `src_format`, into a qcow2 at `dest`.
///
/// `src_format` must be the format [`supplied_image_format`] reported, and must not be qcow2 —
/// [`refuse_chain_flattening`] rejects that argv before `qemu-img` is spawned, because a qcow2
/// source is the flattening case rather than the normalising one.
///
/// Blocking, because the import path it serves ([`crate::library::VmLibrary::import_base_image`])
/// is called from synchronous CLI and testkit code.
pub fn normalise_to_qcow2(src: &Path, src_format: &str, dest: &Path) -> Result<(), VmError> {
    let args = convert_argv(src, src_format, dest);
    run_qemu_img_blocking(&args).map_err(VmError::BuildFailed)
}

/// `qemu-img convert -f <src_format> -O qcow2 <src> <dest>`.
fn convert_argv(src: &Path, src_format: &str, dest: &Path) -> Vec<String> {
    vec![
        "convert".to_string(),
        "-f".to_string(),
        src_format.to_string(),
        "-O".to_string(),
        QCOW2_FORMAT.to_string(),
        src.display().to_string(),
        dest.display().to_string(),
    ]
}

/// Refuse a `qemu-img` argv that would flatten a backing chain, naming the argv it rejected.
///
/// A `convert` is refused when its input format is qcow2, and equally when it names no input
/// format at all: without `-f`, `qemu-img` probes the source, so an unnamed format is a
/// flattening that has merely not been written down yet. Every other argv — `create`, `info`,
/// a convert of a raw or VMDK source — passes.
pub fn refuse_chain_flattening(args: &[String]) -> Result<(), String> {
    if args.first().map(String::as_str) != Some("convert") {
        return Ok(());
    }
    match input_format(args) {
        Some(QCOW2_FORMAT) => Err(format!(
            "refusing qemu-img {args:?}: converting a qcow2 source reads its whole backing \
             chain and writes one standalone image, silently turning a delta into a full copy \
             and severing its parent"
        )),
        Some(_) => Ok(()),
        None => Err(format!(
            "refusing qemu-img {args:?}: a convert must name its input format with -f, since a \
             probed source may be a qcow2 whose backing chain would be flattened"
        )),
    }
}

/// The value of the `-f` (input format) flag in `args`, if it has one.
fn input_format(args: &[String]) -> Option<&str> {
    args.windows(2)
        .find(|pair| pair[0] == "-f")
        .map(|pair| pair[1].as_str())
}

/// Run `qemu-img` with `args` and block until it exits, surfacing stderr on a non-zero exit.
///
/// The blocking counterpart of [`crate::cloud_init::run_qemu_img`], and subject to the same
/// [`refuse_chain_flattening`] check — a second runner must not become a second way to flatten
/// a chain.
fn run_qemu_img_blocking(args: &[String]) -> Result<(), String> {
    refuse_chain_flattening(args)?;
    let output = std::process::Command::new("qemu-img")
        .args(args)
        .output()
        .map_err(|e| format!("qemu-img launch failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "qemu-img {} failed: {}",
            args.first().map(String::as_str).unwrap_or(""),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

/// How much of each file [`files_have_identical_content`] holds in memory at a time.
const COMPARE_CHUNK_BYTES: usize = 64 * 1024;

/// Whether `a` and `b` hold exactly the same bytes.
///
/// Compares content, not length: two images of identical size and different content are the
/// dangerous case an import must catch, since qcow2 records no identity of its parent and a
/// silent swap re-parents every layer chained onto it. Streamed in
/// [`COMPARE_CHUNK_BYTES`] chunks — a base image is hundreds of megabytes, and neither of them
/// needs to be resident to answer this.
pub(crate) fn files_have_identical_content(a: &Path, b: &Path) -> Result<bool, VmError> {
    if file_len(a)? != file_len(b)? {
        return Ok(false);
    }

    let mut a_reader = BufReader::new(open_for_read(a)?);
    let mut b_reader = BufReader::new(open_for_read(b)?);
    let mut a_chunk = vec![0u8; COMPARE_CHUNK_BYTES];
    let mut b_chunk = vec![0u8; COMPARE_CHUNK_BYTES];
    loop {
        let read = a_reader
            .read(&mut a_chunk)
            .map_err(|e| read_failed(a, &e.to_string()))?;
        if read == 0 {
            return Ok(true);
        }
        // The two lengths agreed above, so `b` still holds at least this many bytes.
        b_reader
            .read_exact(&mut b_chunk[..read])
            .map_err(|e| read_failed(b, &e.to_string()))?;
        if a_chunk[..read] != b_chunk[..read] {
            return Ok(false);
        }
    }
}

fn file_len(path: &Path) -> Result<u64, VmError> {
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| read_failed(path, &e.to_string()))
}

fn open_for_read(path: &Path) -> Result<std::fs::File, VmError> {
    std::fs::File::open(path).map_err(|e| read_failed(path, &e.to_string()))
}

fn read_failed(path: &Path, reason: &str) -> VmError {
    VmError::BuildFailed(format!("failed to read {}: {reason}", path.display()))
}

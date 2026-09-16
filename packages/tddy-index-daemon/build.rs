//! Codegen for `code_index.proto`, in the two passes `tddy-terminal-rpc` established:
//!
//! - a prost pass carrying the `tddy-rpc` `RpcService` server (what stdio and any other
//!   `tddy-rpc` transport register) plus the tonic adapter that delegates one implementation of
//!   the generated trait to the tonic server trait emitted below;
//! - a tonic pass (gRPC server + client) reusing the canonical prost message types via
//!   `extern_path`, so both trait impls speak identical Rust types and nothing is re-encoded when
//!   a request crosses from one transport to the other.
//!
//! The two passes each emit a `CodeIndexService` trait, so the tonic one lands in its own module
//! (`proto::tonic_code_index`) and the adapter reaches it through `tonic_trait_path` rather than an
//! unqualified import.
//!
//! A third, descriptor-only pass writes the file-descriptor set, so a host can merge this service
//! into gRPC reflection without re-parsing the proto.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = std::env::var("OUT_DIR")?;

    // RpcService-flavoured pass: the async trait, the `RpcService` server, and the tonic adapter.
    prost_build::Config::new()
        .out_dir(&out_dir)
        .service_generator(Box::new(tddy_codegen::TddyServiceGenerator {
            generate_rpc_server: true,
            generate_tonic_adapter: true,
            rpc_crate_path: "tddy_rpc".to_string(),
            tonic_trait_path: Some(
                "crate::proto::tonic_code_index::code_index_service_server".to_string(),
            ),
        }))
        .compile_protos(&["proto/code_index.proto"], &["proto"])?;

    // Tonic gRPC server/client over the same message types. `src/lib.rs` includes this output —
    // without that include the adapter's `tonic_trait_path` would not resolve and the gRPC server
    // trait would be generated but unreachable.
    let tonic_dir = format!("{out_dir}/tonic_code_index");
    std::fs::create_dir_all(&tonic_dir)?;
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .out_dir(&tonic_dir)
        .extern_path(".code_index", "crate::proto::code_index")
        .compile_protos(&["proto/code_index.proto"], &["proto"])?;

    // Descriptor set only. Its own scratch out_dir: this pass would otherwise write a second
    // `code_index.rs` over the first pass's.
    let descriptor_path = format!("{out_dir}/code_index_descriptors.bin");
    let descriptor_scratch = format!("{out_dir}/descriptor_set_only");
    std::fs::create_dir_all(&descriptor_scratch)?;
    prost_build::Config::new()
        .file_descriptor_set_path(&descriptor_path)
        .out_dir(&descriptor_scratch)
        .compile_protos(&["proto/code_index.proto"], &["proto"])?;

    Ok(())
}

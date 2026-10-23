//! The WASI preview 2 subset a `wasm32-wasip2` extension imports, answered by hand.
//!
//! `wasmtime-wasi` would give these interfaces whole, and is a dependency nobody has consented to
//! (see `wit/wasi-0.2.6-subset/README.md`). What an extension built with `zed_extension_api` and
//! Rust's standard library needs from them is small, and every answer here is the least that does
//! not make it trap:
//!
//! - **environment** — `PWD=/`, which `init-extension` reads and `set_current_dir`s to; nothing
//!   from this process's environment ever reaches an extension;
//! - **filesystem** — one preopened directory, `/`, which is empty and read-only: `set_current_dir`
//!   needs a directory to enter, and the extension reaches no file of this machine's;
//! - **stdio** — stdout and stderr accept and discard what is written (an extension's `eprintln!`
//!   must not reach the TUI); stdin is empty; none is a terminal;
//! - **exit** — a trap naming the status: an extension may not end this process's call quietly.
//!
//! The functions are `trappable`, as the Zed imports are: an `Err` is a trap, an `Ok(Err(_))` is
//! the interface's own error.

use wasmtime::component::Resource;

use crate::wasm::imports::HostState;

pub(crate) mod bindings {
    wasmtime::component::bindgen!({
        path: "wit/wasi-0.2.6-subset",
        world: "tddy:wasi-subset/wasi-subset",
        imports: { default: trappable },
    });
}

use bindings::wasi::cli::{
    environment, exit, stderr, stdin, stdout, terminal_input, terminal_output, terminal_stderr,
    terminal_stdin, terminal_stdout,
};
use bindings::wasi::clocks::wall_clock;
use bindings::wasi::filesystem::{preopens, types as fs};
use bindings::wasi::io::{error as io_error, poll, streams};

/// The working directory an extension is told it has.
const PWD: &str = "/";

/// How much a stream says it will accept in one write: it accepts everything.
const WRITABLE: u64 = 4096;

impl poll::Host for HostState {}
impl io_error::Host for HostState {}
impl streams::Host for HostState {}
impl terminal_input::Host for HostState {}
impl terminal_output::Host for HostState {}
impl wall_clock::Host for HostState {}
impl fs::Host for HostState {}

impl poll::HostPollable for HostState {
    /// Every stream here is always ready.
    fn block(&mut self, _pollable: Resource<poll::Pollable>) -> wasmtime::Result<()> {
        Ok(())
    }

    fn drop(&mut self, _pollable: Resource<poll::Pollable>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl io_error::HostError for HostState {
    fn drop(&mut self, _error: Resource<io_error::Error>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl streams::HostInputStream for HostState {
    fn drop(&mut self, _stream: Resource<streams::InputStream>) -> wasmtime::Result<()> {
        Ok(())
    }
}

/// Accepts and discards.
impl streams::HostOutputStream for HostState {
    fn check_write(
        &mut self,
        _stream: Resource<streams::OutputStream>,
    ) -> wasmtime::Result<Result<u64, streams::StreamError>> {
        Ok(Ok(WRITABLE))
    }

    fn write(
        &mut self,
        _stream: Resource<streams::OutputStream>,
        _contents: Vec<u8>,
    ) -> wasmtime::Result<Result<(), streams::StreamError>> {
        Ok(Ok(()))
    }

    fn blocking_flush(
        &mut self,
        _stream: Resource<streams::OutputStream>,
    ) -> wasmtime::Result<Result<(), streams::StreamError>> {
        Ok(Ok(()))
    }

    fn subscribe(
        &mut self,
        _stream: Resource<streams::OutputStream>,
    ) -> wasmtime::Result<Resource<poll::Pollable>> {
        Ok(Resource::new_own(0))
    }

    fn drop(&mut self, _stream: Resource<streams::OutputStream>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl terminal_input::HostTerminalInput for HostState {
    fn drop(&mut self, _terminal: Resource<terminal_input::TerminalInput>) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl terminal_output::HostTerminalOutput for HostState {
    fn drop(
        &mut self,
        _terminal: Resource<terminal_output::TerminalOutput>,
    ) -> wasmtime::Result<()> {
        Ok(())
    }
}

impl environment::Host for HostState {
    fn get_environment(&mut self) -> wasmtime::Result<Vec<(String, String)>> {
        Ok(vec![("PWD".to_string(), PWD.to_string())])
    }
}

impl exit::Host for HostState {
    fn exit(&mut self, status: Result<(), ()>) -> wasmtime::Result<()> {
        Err(anyhow::anyhow!(
            "extension {} called exit({})",
            self.extension,
            if status.is_ok() { "ok" } else { "error" }
        ))
    }
}

impl stdin::Host for HostState {
    fn get_stdin(&mut self) -> wasmtime::Result<Resource<streams::InputStream>> {
        Ok(Resource::new_own(0))
    }
}

impl stdout::Host for HostState {
    fn get_stdout(&mut self) -> wasmtime::Result<Resource<streams::OutputStream>> {
        Ok(Resource::new_own(0))
    }
}

impl stderr::Host for HostState {
    fn get_stderr(&mut self) -> wasmtime::Result<Resource<streams::OutputStream>> {
        Ok(Resource::new_own(0))
    }
}

impl terminal_stdin::Host for HostState {
    fn get_terminal_stdin(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_input::TerminalInput>>> {
        Ok(None)
    }
}

impl terminal_stdout::Host for HostState {
    fn get_terminal_stdout(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_output::TerminalOutput>>> {
        Ok(None)
    }
}

impl terminal_stderr::Host for HostState {
    fn get_terminal_stderr(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_output::TerminalOutput>>> {
        Ok(None)
    }
}

impl preopens::Host for HostState {
    /// One directory, [`PWD`], which holds nothing: an extension reaches no file of this machine's.
    fn get_directories(&mut self) -> wasmtime::Result<Vec<(Resource<fs::Descriptor>, String)>> {
        Ok(vec![(Resource::new_own(0), PWD.to_string())])
    }
}

/// The one directory [`preopens::Host::get_directories`] hands out: empty and read-only. It can be
/// told apart from nothing — `chdir` into it works — and holds no file, so every path below it is
/// absent and nothing can be opened, read or written.
impl fs::HostDescriptor for HostState {
    fn read_via_stream(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
        _offset: fs::Filesize,
    ) -> wasmtime::Result<Result<Resource<streams::InputStream>, fs::ErrorCode>> {
        Ok(Err(fs::ErrorCode::IsDirectory))
    }

    fn write_via_stream(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
        _offset: fs::Filesize,
    ) -> wasmtime::Result<Result<Resource<streams::OutputStream>, fs::ErrorCode>> {
        Ok(Err(fs::ErrorCode::ReadOnly))
    }

    fn append_via_stream(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
    ) -> wasmtime::Result<Result<Resource<streams::OutputStream>, fs::ErrorCode>> {
        Ok(Err(fs::ErrorCode::ReadOnly))
    }

    fn get_flags(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
    ) -> wasmtime::Result<Result<fs::DescriptorFlags, fs::ErrorCode>> {
        Ok(Ok(fs::DescriptorFlags::READ))
    }

    fn stat(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
    ) -> wasmtime::Result<Result<fs::DescriptorStat, fs::ErrorCode>> {
        Ok(Ok(the_directory()))
    }

    fn stat_at(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
        _path_flags: fs::PathFlags,
        path: String,
    ) -> wasmtime::Result<Result<fs::DescriptorStat, fs::ErrorCode>> {
        Ok(if is_the_directory(&path) {
            Ok(the_directory())
        } else {
            Err(fs::ErrorCode::NoEntry)
        })
    }

    fn metadata_hash(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
    ) -> wasmtime::Result<Result<fs::MetadataHashValue, fs::ErrorCode>> {
        Ok(Ok(THE_DIRECTORY_HASH))
    }

    fn metadata_hash_at(
        &mut self,
        _descriptor: Resource<fs::Descriptor>,
        _path_flags: fs::PathFlags,
        path: String,
    ) -> wasmtime::Result<Result<fs::MetadataHashValue, fs::ErrorCode>> {
        Ok(if is_the_directory(&path) {
            Ok(THE_DIRECTORY_HASH)
        } else {
            Err(fs::ErrorCode::NoEntry)
        })
    }

    fn drop(&mut self, _descriptor: Resource<fs::Descriptor>) -> wasmtime::Result<()> {
        Ok(())
    }
}

/// What identifies the directory: there is only one.
const THE_DIRECTORY_HASH: fs::MetadataHashValue = fs::MetadataHashValue { lower: 0, upper: 0 };

fn the_directory() -> fs::DescriptorStat {
    fs::DescriptorStat {
        type_: fs::DescriptorType::Directory,
        link_count: 1,
        size: 0,
        data_access_timestamp: None,
        data_modification_timestamp: None,
        status_change_timestamp: None,
    }
}

/// Whether `path`, relative to the directory, is the directory itself.
fn is_the_directory(path: &str) -> bool {
    matches!(path, "" | "." | "/")
}

//! Tddy Desktop: `tddy-daemon` and its dashboard in a single process.
//!
//! This is the daemon's second host. `tddy-daemon`'s `main` assembles the roster with
//! [`tddy_daemon::runtime::build`] and serves it over HTTP; this application assembles the same
//! roster and serves it over the webview's IPC bridge instead. Nothing listens on a TCP port —
//! that is the point: the dashboard reaches the daemon over a channel no other local process can
//! address.
//!
//! The startup order is load-bearing. The spawn worker is forked before any async runtime exists,
//! because `fork` from a multi-threaded process can deadlock, and Tauri's runtime is
//! multi-threaded from the moment it is touched.

mod config_source;
mod ipc;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Manager, RunEvent, Url, WebviewUrl, WebviewWindowBuilder};
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::runtime::{self, RuntimeOptions, RuntimeTaskHandles};
use tddy_rpc::MultiRpcService;
use tddy_tauri_rpc::WebviewRpcHost;

/// The window the dashboard is loaded into.
const MAIN_WINDOW_LABEL: &str = "main";

/// What the application has to stop when it exits: the daemon's own children, and the tasks its
/// runtime started. The `tddy-daemon` binary does exactly this on `SIGTERM` — a claude-cli session
/// outlives the RPC surface otherwise, and an orphaned one survives the app that started it.
struct DaemonShutdown {
    cli_sessions: Arc<CliSessionManager>,
    tasks: RuntimeTaskHandles,
}

/// Start the application: assemble the daemon, host it for the webview, and open the dashboard.
pub fn run() -> anyhow::Result<()> {
    // Ignore SIGPIPE — writing to the spawn worker pipe after it dies would otherwise kill the app.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_IGN);
    }

    let source = config_source::resolve()?;
    let config = source.load_config()?;

    let log_config = config
        .log
        .clone()
        .unwrap_or_else(|| tddy_core::default_log_config(None, None));
    tddy_core::init_tddy_logger(log_config);
    log::info!(
        "[tddy-desktop] hosting tddy-daemon from {} (workspace {})",
        source.config_path.display(),
        source.workspace_root.display()
    );

    // Fork the spawn worker before anything starts a runtime: `fork` from a multi-threaded process
    // can deadlock, and every line below this one may touch Tauri's. Skipped on a supervised host,
    // where `tddy-supervisor` spawns sessions instead.
    let spawn_backend = tddy_daemon::supervisor_client::spawn_backend_choice(&config);
    let spawn_client = tddy_daemon::supervisor_client::spawn_worker_for(&spawn_backend)?;
    #[cfg(unix)]
    if let Some((_, worker_pid)) = spawn_client.as_ref() {
        log::info!("[tddy-desktop] spawn worker pid={worker_pid}");
    }

    // Scope git's ssh command to this daemon, without polluting the process environment or the
    // global git config. See DaemonConfig::git / GitConfig::ssh_command.
    tddy_core::set_git_ssh_command(config.git.as_ref().and_then(|g| g.ssh_command.clone()));

    let dashboard = dashboard_url()?;
    let config_path = source.config_path;
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ipc::tddy_rpc_connect,
            ipc::tddy_rpc_send
        ])
        .setup(move |app| {
            start_daemon(app, config, config_path, spawn_client)?;
            open_dashboard_window(app, dashboard)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    app.run(|app, event| {
        if matches!(event, RunEvent::Exit) {
            stop_daemon(app);
        }
    });
    Ok(())
}

/// Assemble the daemon, hand its roster to the webview host, and start what the runtime left to
/// its host to start.
fn start_daemon(
    app: &tauri::App,
    config: tddy_daemon::config::DaemonConfig,
    config_path: PathBuf,
    spawn_client: Option<(tddy_daemon::spawn_worker::SpawnClient, i32)>,
) -> anyhow::Result<()> {
    let handle = app.handle().clone();
    // `build` is assembly, `spawn` needs a runtime context, and so does the signal listener, so all
    // three run on Tauri's — the same runtime the two IPC commands are dispatched on, which is what
    // lets a response the daemon produces reach the sink a command registered.
    let (state, shutdown) = tauri::async_runtime::block_on(async move {
        let daemon = runtime::build(
            config,
            RuntimeOptions::for_embedded()
                .with_config_path(Some(config_path))
                .with_spawn_worker(spawn_client),
        )
        .await?;
        log::info!(
            "[tddy-desktop] daemon assembled with {} services",
            daemon.entries.len()
        );
        // TODO: the binary host sends "started"/"stopped" Telegram lifecycle messages from
        // `server::run_server`; this host does not, because window creation would then wait on a
        // Telegram HTTP call. Move the lifecycle message out of the HTTP server to share it.
        let state = ipc::RpcState::new(WebviewRpcHost::new(MultiRpcService::new(daemon.entries)));
        let shutdown = DaemonShutdown {
            cli_sessions: daemon.cli_sessions,
            tasks: daemon.tasks.spawn(),
        };
        #[cfg(unix)]
        exit_on_sigterm(&handle)?;
        Ok::<_, anyhow::Error>((state, shutdown))
    })?;

    app.manage(state);
    app.manage(shutdown);
    Ok(())
}

/// Ask the application to exit when a service manager (or a logout) sends `SIGTERM`, so the window
/// closes and [`stop_daemon`] runs.
///
/// Listening for the signal is also what takes it off its default disposition, so once this is
/// registered nothing else ends this process — which is exactly why registration failing is a
/// startup error rather than something to carry on without.
#[cfg(unix)]
fn exit_on_sigterm(app: &tauri::AppHandle) -> anyhow::Result<()> {
    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        sigterm.recv().await;
        log::info!("[tddy-desktop] SIGTERM received — shutting the daemon down");
        app.exit(0);
    });
    Ok(())
}

/// Kill the daemon's cli sessions and abort its runtime tasks, the way the binary does on SIGTERM.
fn stop_daemon(app: &tauri::AppHandle) {
    let Some(shutdown) = app.try_state::<DaemonShutdown>() else {
        return;
    };
    tauri::async_runtime::block_on(shutdown.cli_sessions.kill_all());
    shutdown.tasks.abort_all();
    log::info!("[tddy-desktop] daemon stopped");
}

/// Where the dashboard is loaded from: a Vite dev server when `VITE_URL` names one, otherwise the
/// built `tddy-web` bundle over Tauri's asset protocol. No HTTP listener either way.
///
/// A debug build embeds no bundle (`tauri.conf.json`'s `devUrl` stands in for it, which is what
/// keeps `cargo build --workspace` from requiring `packages/tddy-web/dist`), so `WebviewUrl::App`
/// resolves against that dev server there and against the asset protocol in a bundled build.
fn dashboard_url() -> anyhow::Result<WebviewUrl> {
    match vite_url()? {
        Some(url) => Ok(WebviewUrl::External(url)),
        None => Ok(WebviewUrl::App("index.html".into())),
    }
}

/// The dev server this application was told to load, if any.
fn vite_url() -> anyhow::Result<Option<Url>> {
    let Ok(value) = std::env::var("VITE_URL") else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let url = Url::parse(trimmed)
        .map_err(|error| anyhow::anyhow!("VITE_URL is not a URL: {trimmed}: {error}"))?;
    Ok(Some(url))
}

/// Open the dashboard window, sending links that leave it to the operator's browser.
fn open_dashboard_window(app: &tauri::App, dashboard: WebviewUrl) -> anyhow::Result<()> {
    let page_origin = match &dashboard {
        WebviewUrl::External(url) => Some(url.clone()),
        _ => None,
    };
    let handle = app.handle().clone();
    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, dashboard)
        .title("Tddy Desktop")
        .inner_size(1280.0, 800.0)
        .position(120.0, 80.0)
        .on_navigation(move |url| {
            if !leaves_the_dashboard(url, page_origin.as_ref()) {
                return true;
            }
            open_in_system_browser(&handle, url.as_str());
            false
        })
        .build()?;
    Ok(())
}

/// Whether `url` is somewhere other than the dashboard — a documentation link, a GitHub PR, an
/// OAuth consent screen — and therefore belongs in the operator's browser rather than in a window
/// whose only job is the dashboard.
fn leaves_the_dashboard(url: &Url, page_origin: Option<&Url>) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    // The asset protocol reaches the bundle as `http://tauri.localhost` on some platforms and
    // `tauri://localhost` on others; both are this application's own page.
    if matches!(url.host_str(), Some("tauri.localhost") | Some("localhost"))
        && page_origin.is_none()
    {
        return false;
    }
    match page_origin {
        Some(origin) => url.origin() != origin.origin(),
        None => true,
    }
}

/// Hand `url` to the operator's browser, reporting a refusal rather than swallowing it — a link
/// that silently does nothing is indistinguishable from a broken dashboard.
fn open_in_system_browser(app: &tauri::AppHandle, url: &str) {
    use tauri_plugin_opener::OpenerExt;

    if let Err(error) = app.opener().open_url(url, None::<&str>) {
        log::warn!("[tddy-desktop] could not open {url} in the system browser: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const THE_DEV_SERVER: &str = "http://localhost:5173";

    fn url(value: &str) -> Url {
        Url::parse(value).expect("a test url")
    }

    #[rstest]
    #[case::a_page_of_the_dev_server("http://localhost:5173/sessions/7", Some(THE_DEV_SERVER))]
    #[case::the_bundle_over_the_asset_protocol("http://tauri.localhost/index.html", None)]
    #[case::the_bundle_over_the_custom_scheme("tauri://localhost/index.html", None)]
    fn keeps_the_dashboards_own_pages_in_the_window(
        #[case] target: &str,
        #[case] dev_server: Option<&str>,
    ) {
        // Given a page the dashboard itself serves
        let dev_server = dev_server.map(url);

        // When the webview is asked to navigate to it
        let leaves = leaves_the_dashboard(&url(target), dev_server.as_ref());

        // Then it stays in the window
        assert!(!leaves, "{target} was sent out of the window");
    }

    #[rstest]
    #[case::in_dev("https://github.com/tddy/tddy-coder/pull/1", Some(THE_DEV_SERVER))]
    #[case::in_a_bundle("https://github.com", None)]
    fn sends_an_external_site_to_the_system_browser(
        #[case] target: &str,
        #[case] dev_server: Option<&str>,
    ) {
        // Given a site the dashboard does not serve
        let dev_server = dev_server.map(url);

        // When the webview is asked to navigate to it
        let leaves = leaves_the_dashboard(&url(target), dev_server.as_ref());

        // Then it opens outside the app rather than replacing the dashboard
        assert!(leaves, "{target} was kept in the window");
    }

    #[test]
    fn leaves_a_non_web_scheme_to_the_webview() {
        // Given a target that is not a web page at all

        // When the webview is asked to navigate to it
        let leaves = leaves_the_dashboard(&url("mailto:someone@example.com"), None);

        // Then it is not treated as an external site — the webview handles it
        assert!(!leaves, "a mailto link was sent to the system browser");
    }
}

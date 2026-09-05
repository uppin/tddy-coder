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
mod oauth_callback;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::webview::{PageLoadEvent, PageLoadPayload};
use tauri::{Manager, RunEvent, Url, Webview, WebviewUrl, WebviewWindowBuilder};
use tddy_daemon::cli_session_manager::CliSessionManager;
use tddy_daemon::runtime::{self, RuntimeOptions, RuntimeTaskHandles};
use tddy_rpc::MultiRpcService;
use tddy_tauri_rpc::MultiConnectionHost;

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
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());

    // Only in a `--features wdio` build, which is the e2e suite's. The plugin serves WebDriver
    // from inside this process — a listening socket that can drive the UI and invoke commands —
    // so it must never reach a shipped bundle. Compiled out entirely rather than disabled at
    // runtime: there is no configuration of a release build that can turn it on.
    #[cfg(feature = "wdio")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());

    let app = builder
        .invoke_handler(tauri::generate_handler![
            ipc::tddy_rpc_connect,
            ipc::tddy_rpc_send,
            ipc::tddy_rpc_disconnect
        ])
        .on_page_load(reap_the_departing_pages_connections)
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

/// Release every connection the page that is being replaced had open.
///
/// A page owns its connections: it mints an epoch per transport and holds the daemon connection
/// plus one per attached session. When that page goes — a reload, or the navigation a completed
/// sign-in performs — it takes the memory of those epochs with it, so it can no longer release
/// them and nothing else knows they exist. Left alone they linger: the host only notices a departed
/// page lazily, when a response it can no longer deliver is published, which for an idle connection
/// is never. One slot used to make this automatic, because `connect` overwrote it; a map of
/// connections has to be told.
///
/// **Ordering is the whole difficulty**, because this fires on the *arriving* page's load, not the
/// departing one's: reaping late would take out the connections the new page has just opened.
/// [`PageLoadEvent::Started`] is the commit of the new document — before its scripts are injected
/// and therefore before it can invoke anything — and the reap is awaited here rather than spawned,
/// so it has finished by the time this returns and the new page begins to run. The work is the
/// daemon's runtime's, not this thread's; what is awaited is its completion.
fn reap_the_departing_pages_connections(webview: &Webview, page: &PageLoadPayload<'_>) {
    // `Finished` is the arriving page already running — by then it has opened its own connections,
    // and reaping would take them with it.
    if !matches!(page.event(), PageLoadEvent::Started) {
        return;
    }
    // Connections are not attributed to a webview by the host, and only the dashboard opens any, so
    // the reap is scoped to the one window that owns them rather than to whatever loaded a page.
    if webview.label() != MAIN_WINDOW_LABEL {
        return;
    }
    // Before the daemon is assembled there is no host and nothing to reap — the dashboard's very
    // first load reaches here, and it precedes every connection there has ever been.
    let Some(state) = webview.try_state::<ipc::RpcState>() else {
        return;
    };
    log::debug!("[tddy-desktop] a new page committed; releasing what the previous one held");
    tauri::async_runtime::block_on(state.disconnect_all());
}

/// Assemble the daemon, hand its roster to the webview host, and start what the runtime left to
/// its host to start.
/// Where a GitHub sign-in should come back to: this process, on loopback.
///
/// `redirect_uri` is derived from `WEB_PUBLIC_URL` / `listen` for a *served* daemon, which is an
/// address a browser on this machine may not reach — and must not be, since the callback carries an
/// authorization code. A desktop sign-in has to come back here.
///
/// Returned rather than written into the configuration: the configuration is what a settings update
/// persists, so a host-chosen address stored there would put a value in the operator's file that
/// they never set, and send a browser sign-in to loopback on some other machine.
fn oauth_callback_address(
    config: &tddy_daemon::config::DaemonConfig,
) -> Option<std::net::SocketAddr> {
    config.github.as_ref()?;
    let port = config.listen.web_port.unwrap_or(DEFAULT_CALLBACK_PORT);
    Some(std::net::SocketAddr::from((
        std::net::Ipv4Addr::LOCALHOST,
        port,
    )))
}

/// Wait for a GitHub sign-in to come back, and send the dashboard to its own callback route.
///
/// The daemon's answer to this is its web server, which this application does not run. What is
/// opened here serves one path and closes as soon as a sign-in completes.
///
/// The dashboard's existing `/auth/callback` route performs the exchange, so no part of the sign-in
/// is re-implemented: the code is carried from the browser back into the window, and the same code
/// path a browser tab uses takes it from there.
fn complete_sign_in_when_the_browser_comes_back(app: &tauri::App, address: std::net::SocketAddr) {
    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let params = match oauth_callback::await_callback(address).await {
            Ok(params) => params,
            Err(error) => {
                log::error!("[tddy-desktop] the sign-in callback could not be served: {error}");
                return;
            }
        };
        let Some(window) = handle.get_webview_window(MAIN_WINDOW_LABEL) else {
            log::error!("[tddy-desktop] a sign-in came back with no window to complete it in");
            return;
        };
        match callback_route(&params) {
            Ok(route) => match window.navigate(route) {
                Ok(()) => {
                    log::info!("[tddy-desktop] sign-in came back; completing it in the window")
                }
                Err(error) => {
                    log::error!("[tddy-desktop] could not open the callback route: {error}")
                }
            },
            Err(error) => log::error!("[tddy-desktop] the callback route is not a url: {error}"),
        }
    });
}

/// The dashboard's own callback route, carrying what the browser brought back.
fn callback_route(params: &oauth_callback::CallbackParams) -> anyhow::Result<Url> {
    let base = match dashboard_url()? {
        WebviewUrl::External(url) => url.to_string(),
        _ => "tauri://localhost/".to_string(),
    };
    let mut route = Url::parse(base.trim_end_matches('/'))?.join("/auth/callback")?;
    route
        .query_pairs_mut()
        .append_pair("code", &params.code)
        .append_pair("state", &params.state);
    Ok(route)
}

/// Where the sign-in callback is served when the configuration names no listen port.
const DEFAULT_CALLBACK_PORT: u16 = 8899;

fn start_daemon(
    app: &tauri::App,
    config: tddy_daemon::config::DaemonConfig,
    config_path: PathBuf,
    spawn_client: Option<(tddy_daemon::spawn_worker::SpawnClient, i32)>,
) -> anyhow::Result<()> {
    let handle = app.handle().clone();
    // Loopback only, and served by this process: an address configured for a *served* daemon (a LAN
    // address out of `WEB_PUBLIC_URL`) is one a browser on this machine may not reach, and the
    // callback carries an authorization code that must not go on the network.
    let callback = oauth_callback_address(&config);
    if let Some(address) = callback {
        log::info!(
            "[tddy-desktop] GitHub sign-in will come back to http://{address}/auth/callback"
        );
        complete_sign_in_when_the_browser_comes_back(app, address);
    }
    // `build` is assembly, `spawn` needs a runtime context, and so does the signal listener, so all
    // three run on Tauri's — the same runtime the two IPC commands are dispatched on, which is what
    // lets a response the daemon produces reach the sink a command registered.
    let (state, shutdown) = tauri::async_runtime::block_on(async move {
        let daemon = runtime::build(
            config,
            RuntimeOptions::for_embedded()
                .with_config_path(Some(config_path))
                // Reaches the auth services and stops there — never the configuration a settings
                // update writes back.
                .with_oauth_redirect(
                    callback.map(|address| oauth_callback::callback_url(address.port())),
                )
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
        // The roster behind every connection, whatever it is addressed to. `Arc` because the
        // resolver hands one out per connection and each connection's engine holds it.
        let roster = Arc::new(MultiRpcService::new(daemon.entries));
        let state = ipc::RpcState::new(MultiConnectionHost::new(ipc::DaemonRosters::over(roster)));
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

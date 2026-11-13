use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use tracing_subscriber::EnvFilter;
use translation_inference::error::Error;
use translation_inference::{app, config};

/// How long in-flight requests may finish after a stop signal; below `TimeoutStopSec` in
/// `deploy/translation-inference.service`.
const DRAIN: std::time::Duration = std::time::Duration::from_secs(25);

#[derive(Parser)]
#[command(
    name = "translation-inference",
    version,
    about = "Translate and transcribe through OpenAI-compatible endpoints"
)]
struct Cli {
    /// TOML config file; TRANSLATION_INFERENCE__SECTION__KEY environment variables override it.
    #[arg(long)]
    config: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();
    match run(Cli::parse()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            e.log();
            eprintln!("{}", e.startup_message());
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), Error> {
    let cfg = config::load(cli.config.as_deref())?;
    let bind = cfg.bind;
    let (router, st) = app::build(cfg).await?;
    if !st.ffmpeg {
        tracing::warn!("ffmpeg was not found on PATH; transcription is off until it is installed");
    }
    if st.dep.plain_http {
        tracing::warn!("Plain http is on: access keys and texts travel unencrypted on the network.");
    }
    app::warm_catalog(&st);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| Error::invalid(format!("Cannot listen on {bind}: {e}.")))?;
    tracing::info!("Listening on {}", st.dep.origin);
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Ready]);
    spawn_watchdog();
    let (stopping, mut stopped) = tokio::sync::watch::channel(false);
    let server = axum::serve(listener, router.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(async move {
            shutdown().await;
            let _ = stopping.send(true);
        });
    // Requests still running after DRAIN (a long file can take an hour) are cut off, inside the
    // unit's TimeoutStopSec.
    let deadline = async move {
        let _ = stopped.wait_for(|s| *s).await;
        tokio::time::sleep(DRAIN).await;
    };
    tokio::select! {
        served = std::future::IntoFuture::into_future(server) => served.map_err(Error::internal),
        () = deadline => {
            tracing::warn!(seconds = DRAIN.as_secs(), "requests were still running after the drain time; stopping anyway");
            Ok(())
        }
    }
}

fn spawn_watchdog() {
    let Some(every) = sd_notify::watchdog_enabled() else { return };
    let every = every / 2;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(every).await;
            let _ = sd_notify::notify(&[sd_notify::NotifyState::Watchdog]);
        }
    });
}

async fn shutdown() {
    let ctrl_c = tokio::signal::ctrl_c();
    let Ok(mut term) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) else {
        return std::future::pending().await;
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = term.recv() => {},
    }
    let _ = sd_notify::notify(&[sd_notify::NotifyState::Stopping]);
}

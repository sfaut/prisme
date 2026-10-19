//! Entry point of Prisme.
//!
//! Reads the configuration, announces the version of Prisme, prepares the
//! data folder (master key and internal database), settles the listening
//! port and the single user of the workstation mode, asked on first launch
//! (`setup.rs`), then starts the web server: the API and the interface
//! (`web_assets.rs`). See `docs/V1.md`, section 2.8.

mod analyze;
mod api;
mod api_error;
mod auth;
mod config;
mod connections;
mod editors;
mod explorer;
mod master_key;
mod profile;
mod query_session;
mod recorder;
mod request_guard;
mod secrets;
mod session_relay;
mod sessions;
mod settings;
mod setup;
mod store;
mod web_assets;

use std::io::IsTerminal;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Log level adjustable through the RUST_LOG variable (e.g. RUST_LOG=debug).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // A panic goes to the logs, with its location and message, before the default
    // hook prints its backtrace on the standard error output.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!("{info}");
        default_hook(info);
    }));

    let config = Config::parse();
    tracing::info!("Prisme v{}", env!("CARGO_PKG_VERSION"));
    config.validate()?;

    let data_dir = config.data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating the data folder {}", data_dir.display()))?;
    tracing::info!("data folder: {}", data_dir.display());

    let master_key = master_key::load_or_create(&data_dir.join(master_key::FILE_NAME))?;

    // At startup, nothing else runs: synchronous access to SQLite is safe here.
    let db_path = data_dir.join(store::FILE_NAME);
    let first_launch = !db_path.exists();
    let db = store::open(&db_path)?;
    tracing::info!("internal database: {}", db_path.display());

    // Port and user: asked on first launch or with --setup, if a terminal
    // allows answering (setup.rs).
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout();
    let interactive = input.is_terminal() && output.is_terminal();
    let terminal = interactive.then_some(setup::Terminal {
        input: &mut input,
        output: &mut output,
        introduced: false,
    });
    let startup = setup::settle(&db, &config, first_launch, terminal, &setup::port_free)?;
    tracing::info!(user_id = startup.user_id, "local user: {}", startup.email);

    let app = api::router(api::AppState::new(db, master_key, startup.user_id));

    let listener = tokio::net::TcpListener::bind(startup.listen)
        .await
        .with_context(|| format!("listening on {}", startup.listen))?;
    tracing::info!("Prisme listens on http://{}", startup.listen);
    axum::serve(listener, app).await?;

    Ok(())
}

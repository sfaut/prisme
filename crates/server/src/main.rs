//! Point d'entrée de Prisme.
//!
//! Lit la configuration, prépare le dossier de données (clef maîtresse et base
//! interne), établit le port d'écoute et l'utilisateur unique du mode poste de
//! développement, demandés au premier lancement (`setup.rs`), puis démarre le
//! serveur web : l'API et l'interface (`web_assets.rs`). Voir
//! `docs/V1.md`, section 2.8.

mod analyze;
mod api;
mod api_error;
mod auth;
mod config;
mod connections;
mod editors;
mod explorer;
mod master_key;
mod query_session;
mod recorder;
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
    // Niveau de journal réglable par la variable RUST_LOG (ex. RUST_LOG=debug).
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::parse();
    config.validate()?;

    let data_dir = config.data_dir()?;
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("création du dossier de données {}", data_dir.display()))?;
    tracing::info!("dossier de données : {}", data_dir.display());

    let master_key = master_key::load_or_create(&data_dir.join(master_key::FILE_NAME))?;

    // Au démarrage, rien d'autre ne tourne : l'accès synchrone à SQLite est sans risque ici.
    let db_path = data_dir.join(store::FILE_NAME);
    let first_launch = !db_path.exists();
    let db = store::open(&db_path)?;
    tracing::info!("base interne : {}", db_path.display());

    // Port et utilisateur : demandés au premier lancement ou avec --setup,
    // si un terminal permet de répondre (setup.rs).
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout();
    let interactive = input.is_terminal() && output.is_terminal();
    let terminal = interactive.then_some(setup::Terminal {
        input: &mut input,
        output: &mut output,
        introduced: false,
    });
    let startup = setup::settle(&db, &config, first_launch, terminal, &setup::port_free)?;
    tracing::info!(
        user_id = startup.user_id,
        "utilisateur local : {}",
        startup.email
    );

    let app = api::router(api::AppState::new(db, master_key, startup.user_id));

    let listener = tokio::net::TcpListener::bind(startup.listen)
        .await
        .with_context(|| format!("écoute sur {}", startup.listen))?;
    tracing::info!("Prisme écoute sur http://{}", startup.listen);
    axum::serve(listener, app).await?;

    Ok(())
}

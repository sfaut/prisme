//! Point d'entrée de Prisme.
//!
//! Lit la configuration, prépare le dossier de données (clef maîtresse et base
//! interne, avec l'utilisateur unique du mode poste de développement), puis
//! démarre le serveur web : l'API et l'interface (`web_assets.rs`). Voir
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
mod settings;
mod store;
mod web_assets;

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
    let db = store::open(&db_path)?;
    tracing::info!("base interne : {}", db_path.display());
    let user_id = store::ensure_user(&db, &config.user_email)?;
    tracing::info!(user_id, "utilisateur local : {}", config.user_email);

    let app = api::router(api::AppState::new(db, master_key, user_id));

    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("écoute sur {}", config.listen))?;
    tracing::info!("Prisme écoute sur http://{}", config.listen);
    axum::serve(listener, app).await?;

    Ok(())
}

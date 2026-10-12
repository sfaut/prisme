//! Point d'entrée de Prisme.
//!
//! Lit la configuration, prépare le dossier de données (clef maîtresse et base
//! interne, avec l'utilisateur unique du mode poste de développement), puis
//! démarre le serveur web. Voir `docs/V1.md`, section 2.8.

mod config;
mod master_key;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "utilisé par l'enregistrement des connexions, incrément suivant"
    )
)]
mod secrets;
mod store;

use anyhow::Context;
use axum::{Router, routing::get};
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

    // La clef servira au chiffrement des connexions (incrément suivant).
    let _master_key = master_key::load_or_create(&data_dir.join(master_key::FILE_NAME))?;

    // Au démarrage, rien d'autre ne tourne : l'accès synchrone à SQLite est sans risque ici.
    let connection = store::open(&data_dir.join(store::FILE_NAME))?;
    let user_id = store::ensure_user(&connection, &config.user_email)?;
    tracing::info!(user_id, "utilisateur local : {}", config.user_email);
    drop(connection);

    let app = Router::new().route("/api/health", get(|| async { "ok" }));

    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .with_context(|| format!("écoute sur {}", config.listen))?;
    tracing::info!("Prisme écoute sur http://{}", config.listen);
    axum::serve(listener, app).await?;

    Ok(())
}

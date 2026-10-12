//! Connecteurs de Prisme : l'accès à chaque moteur de base de données cible.
//!
//! Un module par famille de moteurs, chacun avec son pilote natif (voir
//! `docs/V1.md`, section 3.2). Pour l'instant : MySQL et MariaDB.

pub mod mysql;

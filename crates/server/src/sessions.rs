//! Registre des sessions de requêtes ouvertes, une par éditeur.
//!
//! Une session d'éditeur survit à son WebSocket (`session_relay.rs`) : le
//! registre la retrouve quand la page revient, pour y raccrocher le nouveau
//! WebSocket au lieu d'ouvrir une autre connexion au serveur cible.
//!
//! - Un WebSocket arrive pour un éditeur : si sa session est ouverte, sur la
//!   même connexion, il s'y raccroche ; sinon une session s'ouvre (celle sur
//!   une autre connexion est arrêtée).
//! - Une session sans WebSocket s'arrête d'elle-même après le délai réglé
//!   sur sa connexion, et sort alors du registre.
//! - Fermer l'éditeur, modifier ou supprimer sa connexion arrête la session
//!   aussitôt.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use axum::extract::ws::WebSocket;

use crate::session_relay::SessionHandle;

/// Sessions d'éditeur ouvertes.
#[derive(Default)]
pub struct Sessions {
    /// Session ouverte de chaque éditeur, par identifiant d'éditeur.
    entries: Mutex<HashMap<i64, Entry>>,
    /// Numéro de la prochaine session ouverte.
    next_id: AtomicU64,
}

/// Session ouverte d'un éditeur.
struct Entry {
    /// Numéro de la session, pour qu'une session terminée ne retire pas du
    /// registre celle qui l'a remplacée.
    id: u64,
    /// Connexion de la session.
    connection_id: i64,
    /// Accès à la session.
    handle: SessionHandle,
}

impl Sessions {
    /// Raccroche `socket` à la session de l'éditeur `editor_id` si elle est
    /// ouverte sur la connexion `connection_id`, sinon en ouvre une avec
    /// `open`, qui reçoit le WebSocket et la fonction à appeler à la fin de
    /// la session.
    pub(crate) fn attach(
        self: &Arc<Self>,
        editor_id: i64,
        connection_id: i64,
        socket: WebSocket,
        open: impl FnOnce(WebSocket, Box<dyn FnOnce() + Send + Sync>) -> SessionHandle,
    ) {
        let mut entries = self.lock();
        let mut socket = socket;
        if let Some(entry) = entries.remove(&editor_id) {
            if entry.connection_id == connection_id {
                match entry.handle.attach(socket) {
                    Ok(()) => {
                        entries.insert(editor_id, entry);
                        return;
                    }
                    // Session terminée entre-temps : une autre s'ouvre.
                    Err(returned) => socket = *returned,
                }
            } else {
                entry.handle.stop();
            }
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let registry = Arc::downgrade(self);
        let on_end = Box::new(move || forget(&registry, editor_id, id));
        let handle = open(socket, on_end);
        entries.insert(
            editor_id,
            Entry {
                id,
                connection_id,
                handle,
            },
        );
    }

    /// Arrête la session de l'éditeur `editor_id`, s'il en a une.
    pub(crate) fn stop_editor(&self, editor_id: i64) {
        if let Some(entry) = self.lock().remove(&editor_id) {
            entry.handle.stop();
        }
    }

    /// Arrête les sessions ouvertes sur la connexion `connection_id`.
    pub(crate) fn stop_connection(&self, connection_id: i64) {
        self.lock().retain(|_, entry| {
            let kept = entry.connection_id != connection_id;
            if !kept {
                entry.handle.stop();
            }
            kept
        });
    }

    /// Verrouille le registre ; un verrou empoisonné est repris tel quel (le
    /// registre reste cohérent : chaque modification tient en une opération).
    fn lock(&self) -> MutexGuard<'_, HashMap<i64, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Retire du registre la session `id` de l'éditeur `editor_id`, qui s'arrête,
/// si elle n'a pas déjà été remplacée.
fn forget(registry: &Weak<Sessions>, editor_id: i64, id: u64) {
    let Some(registry) = registry.upgrade() else {
        return;
    };
    let mut entries = registry.lock();
    if entries.get(&editor_id).is_some_and(|entry| entry.id == id) {
        entries.remove(&editor_id);
    }
}

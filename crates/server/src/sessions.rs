//! Registry of the open query sessions, one per editor.
//!
//! An editor's session outlives its WebSocket (`session_relay.rs`): the
//! registry finds it again when the page comes back, to reattach the new
//! WebSocket to it instead of opening another connection to the target
//! server.
//!
//! - A WebSocket arrives for an editor: if its session is open, on the same
//!   connection, it reattaches to it; otherwise a session opens (the one on
//!   another connection is stopped).
//! - A session without a WebSocket stops by itself after the delay set on its
//!   connection, and then leaves the registry.
//! - Closing the editor, editing or deleting its connection stops the session
//!   at once, like closing the session itself ("Déconnecter" of the object
//!   navigator).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use axum::extract::ws::WebSocket;

use crate::session_relay::SessionHandle;

/// Open editor sessions.
#[derive(Default)]
pub struct Sessions {
    /// Open session of each editor, by editor id.
    entries: Mutex<HashMap<i64, Entry>>,
    /// Number of the next session opened.
    next_id: AtomicU64,
}

/// Open session of an editor.
struct Entry {
    /// Number of the session, so that an ended session does not remove from
    /// the registry the one that replaced it.
    id: u64,
    /// Connection of the session.
    connection_id: i64,
    /// Access to the session.
    handle: SessionHandle,
}

impl Sessions {
    /// Reattaches `socket` to the session of editor `editor_id` if it is open
    /// on connection `connection_id`, otherwise opens one with `open`, which
    /// receives the WebSocket and the function to call at the end of the
    /// session.
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
                    // Session ended in the meantime: another one opens.
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

    /// Stops the session of editor `editor_id`, if it has one.
    pub(crate) fn stop_editor(&self, editor_id: i64) {
        if let Some(entry) = self.lock().remove(&editor_id) {
            entry.handle.stop();
        }
    }

    /// Stops the sessions open on connection `connection_id`.
    pub(crate) fn stop_connection(&self, connection_id: i64) {
        self.lock().retain(|_, entry| {
            let kept = entry.connection_id != connection_id;
            if !kept {
                entry.handle.stop();
            }
            kept
        });
    }

    /// Locks the registry; a poisoned lock is taken over as is (the registry
    /// stays consistent: each change fits in a single operation).
    fn lock(&self) -> MutexGuard<'_, HashMap<i64, Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Removes from the registry the session `id` of editor `editor_id`, which is
/// stopping, unless it was already replaced.
fn forget(registry: &Weak<Sessions>, editor_id: i64, id: u64) {
    let Some(registry) = registry.upgrade() else {
        return;
    };
    let mut entries = registry.lock();
    if entries.get(&editor_id).is_some_and(|entry| entry.id == id) {
        entries.remove(&editor_id);
    }
}

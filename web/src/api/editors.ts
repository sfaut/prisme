/**
 * Éditeurs SQL (onglets du haut de l'écran) et leurs onglets de résultat,
 * enregistrés côté serveur (`crates/server/src/editors.rs`).
 */

import { request } from './http'
import type { ClosedEditor } from './generated/ClosedEditor'
import type { Editor } from './generated/Editor'
import type { EditorUpdate } from './generated/EditorUpdate'
import type { ResultTab } from './generated/ResultTab'
import type { ResultTabUpdate } from './generated/ResultTabUpdate'

/**
 * Éditeurs ouverts, dans l'ordre, avec leurs onglets de résultat enregistrés.
 */
export async function listEditors(): Promise<Editor[]> {
  return request<Editor[]>('GET', '/api/editors')
}

/**
 * Crée un éditeur vide, à la suite des autres.
 *
 * @param connectionId Connexion de départ, `null` pour aucune.
 */
export async function createEditor(connectionId: number | null): Promise<Editor> {
  return request<Editor>('POST', '/api/editors', { connection_id: connectionId ?? undefined })
}

/**
 * Enregistre le nom, le texte et la connexion d'un éditeur.
 *
 * @param id Identifiant de l'éditeur.
 * @param editor Nom, texte et connexion.
 */
export async function updateEditor(id: number, editor: EditorUpdate): Promise<void> {
  await request('PUT', `/api/editors/${id}`, editor)
}

/**
 * Ferme un éditeur : il reste rouvrable 24 heures.
 *
 * @param id Identifiant de l'éditeur.
 */
export async function closeEditor(id: number): Promise<void> {
  await request('DELETE', `/api/editors/${id}`)
}

/**
 * Range les éditeurs ouverts dans l'ordre donné.
 *
 * @param ids Identifiants, du premier au dernier.
 */
export async function reorderEditors(ids: number[]): Promise<void> {
  await request('PUT', '/api/editors/order', { ids })
}

/**
 * Éditeurs fermés depuis moins de 24 heures, du plus récent au plus ancien.
 */
export async function listClosedEditors(): Promise<ClosedEditor[]> {
  return request<ClosedEditor[]>('GET', '/api/editors/closed')
}

/**
 * Rouvre un éditeur fermé.
 *
 * @param id Identifiant de l'éditeur.
 */
export async function reopenEditor(id: number): Promise<void> {
  await request('POST', `/api/editors/${id}/reopen`)
}

/**
 * Onglets de résultat enregistrés d'un éditeur, lignes comprises.
 *
 * @param editorId Identifiant de l'éditeur.
 */
export async function listResults(editorId: number): Promise<ResultTab[]> {
  return request<ResultTab[]>('GET', `/api/editors/${editorId}/results`)
}

/**
 * Range les onglets de résultat d'un éditeur dans l'ordre donné.
 *
 * @param editorId Identifiant de l'éditeur.
 * @param ids Identifiants des onglets, du premier au dernier.
 */
export async function reorderResults(editorId: number, ids: number[]): Promise<void> {
  await request('PUT', `/api/editors/${editorId}/results/order`, { ids })
}

/**
 * Renomme ou épingle un onglet de résultat.
 *
 * @param id Identifiant de l'onglet.
 * @param tab Nom et épinglage.
 */
export async function updateResult(id: number, tab: ResultTabUpdate): Promise<void> {
  await request('PUT', `/api/results/${id}`, tab)
}

/**
 * Supprime un onglet de résultat et ses lignes.
 *
 * @param id Identifiant de l'onglet.
 */
export async function deleteResult(id: number): Promise<void> {
  await request('DELETE', `/api/results/${id}`)
}

/**
 * SQL editors (tabs at the top of the screen) and their result tabs, saved on
 * the server side (`crates/server/src/editors.rs`).
 */

import { request } from './http'
import type { ClosedEditor } from './generated/ClosedEditor'
import type { Editor } from './generated/Editor'
import type { EditorUpdate } from './generated/EditorUpdate'
import type { ResultTab } from './generated/ResultTab'
import type { ResultTabUpdate } from './generated/ResultTabUpdate'

/**
 * Open editors, in order, with their saved result tabs.
 */
export async function listEditors(): Promise<Editor[]> {
  return request<Editor[]>('GET', '/api/editors')
}

/**
 * Creates an empty editor, after the others.
 *
 * @param connectionId Initial connection, `null` for none.
 */
export async function createEditor(connectionId: number | null): Promise<Editor> {
  return request<Editor>('POST', '/api/editors', { connection_id: connectionId ?? undefined })
}

/**
 * Saves the name, the text and the connection of an editor.
 *
 * @param id Id of the editor.
 * @param editor Name, text and connection.
 */
export async function updateEditor(id: number, editor: EditorUpdate): Promise<void> {
  await request('PUT', `/api/editors/${id}`, editor)
}

/**
 * Closes an editor: it can be reopened for 24 hours.
 *
 * @param id Id of the editor.
 */
export async function closeEditor(id: number): Promise<void> {
  await request('DELETE', `/api/editors/${id}`)
}

/**
 * Puts the open editors in the given order.
 *
 * @param ids Ids, from the first to the last.
 */
export async function reorderEditors(ids: number[]): Promise<void> {
  await request('PUT', '/api/editors/order', { ids })
}

/**
 * Editors closed less than 24 hours ago, from the most recent to the oldest.
 */
export async function listClosedEditors(): Promise<ClosedEditor[]> {
  return request<ClosedEditor[]>('GET', '/api/editors/closed')
}

/**
 * Reopens a closed editor.
 *
 * @param id Id of the editor.
 */
export async function reopenEditor(id: number): Promise<void> {
  await request('POST', `/api/editors/${id}/reopen`)
}

/**
 * Saved result tabs of an editor, rows included.
 *
 * @param editorId Id of the editor.
 */
export async function listResults(editorId: number): Promise<ResultTab[]> {
  return request<ResultTab[]>('GET', `/api/editors/${editorId}/results`)
}

/**
 * Puts the result tabs of an editor in the given order.
 *
 * @param editorId Id of the editor.
 * @param ids Ids of the tabs, from the first to the last.
 */
export async function reorderResults(editorId: number, ids: number[]): Promise<void> {
  await request('PUT', `/api/editors/${editorId}/results/order`, { ids })
}

/**
 * Renames or pins a result tab.
 *
 * @param id Id of the tab.
 * @param tab Name and pinning.
 */
export async function updateResult(id: number, tab: ResultTabUpdate): Promise<void> {
  await request('PUT', `/api/results/${id}`, tab)
}

/**
 * Deletes a result tab and its rows.
 *
 * @param id Id of the tab.
 */
export async function deleteResult(id: number): Promise<void> {
  await request('DELETE', `/api/results/${id}`)
}

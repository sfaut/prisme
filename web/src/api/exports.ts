/**
 * Exports of full results: the server runs a query again, without the limit
 * of Prisme, and writes its rows to a CSV file for Excel in French, in its
 * exports folder (`docs/DEVELOPER.md`, routes `/api/exports`).
 *
 * The list of the user's exports is read once (`listExports`), with the use
 * of the folder, then followed through server-sent events
 * (`followExports`): one `export` event per export started, progressing
 * (every half second at most), ended or removed; `lagged` when the server
 * dropped events for a client too slow, which then reads the list again.
 *
 * The requests do not show in the synchronisation state of the top bar
 * (`sync: false`): an export works on a target server and on files, and
 * its refusal (a write, two columns of the same name) is no failed save of
 * the user's data.
 */

import { request } from './http'
import type { ExportEvent } from './generated/ExportEvent'
import type { ExportInfo } from './generated/ExportInfo'
import type { ExportList } from './generated/ExportList'
import type { ExportRequest } from './generated/ExportRequest'

/**
 * Starts an export.
 *
 * @param requested Query, columns and name of the file.
 * @returns The export, running, or failed when its file could not be created.
 */
export function startExport(requested: ExportRequest): Promise<ExportInfo> {
  return request('POST', '/api/exports', requested, { sync: false })
}

/**
 * Reads the user's exports, the most recent first, and the use of the
 * exports folder.
 */
export function listExports(): Promise<ExportList> {
  return request('GET', '/api/exports', undefined, { sync: false })
}

/**
 * Stops a running export; its partial file stays.
 *
 * @param id Id of the export.
 */
export function cancelExport(id: number): Promise<void> {
  return request('POST', `/api/exports/${id}/cancel`, undefined, { sync: false })
}

/**
 * Removes an export and deletes its file, after stopping it if it runs.
 *
 * @param id Id of the export.
 */
export function removeExport(id: number): Promise<void> {
  return request('DELETE', `/api/exports/${id}`, undefined, { sync: false })
}

/**
 * Deletes the files of the exports folder that no export references.
 */
export function removeOrphans(): Promise<void> {
  return request('DELETE', '/api/exports/orphans', undefined, { sync: false })
}

/**
 * Address of an export's file, which the browser downloads under the name
 * of the export.
 *
 * @param id Id of the export.
 */
export function exportFileUrl(id: number): string {
  return `/api/exports/${id}/file`
}

/**
 * Receivers of the exports' events.
 */
export interface ExportHandlers {
  /**
   * An export started, progressed, ended or was removed.
   */
  onEvent: (event: ExportEvent) => void
  /**
   * Events were lost, or the stream reopened after a break: the list may
   * be out of date.
   */
  onGap: () => void
}

/**
 * Follows the exports' events until the returned function is called. The
 * browser reopens the stream by itself after a break.
 *
 * @param handlers Receivers of the events.
 * @returns Stops following.
 */
export function followExports(handlers: ExportHandlers): () => void {
  const source = new EventSource('/api/exports/stream')
  let opened = false
  source.addEventListener('open', () => {
    // A reopening may have missed events.
    if (opened) {
      handlers.onGap()
    }
    opened = true
  })
  source.addEventListener('export', (event) => {
    handlers.onEvent(JSON.parse((event as MessageEvent<string>).data) as ExportEvent)
  })
  source.addEventListener('lagged', () => handlers.onGap())
  return () => source.close()
}

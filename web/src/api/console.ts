/**
 * Console: history of the queries sent to the target servers, and debug
 * log (lines of the server's log, notifications of the interface).
 *
 * Both are read by pages, from the most recent (`listHistory`, `listDebug`),
 * then followed as they grow through server-sent events (`followConsole`):
 * one `history` event per new query, one `debug` event per new line,
 * `history-cleared` / `debug-cleared` when a list is cleared (`clearHistory`,
 * `clearDebug`), `lagged` when the server dropped events for a client too
 * slow, which then reads them again.
 *
 * A page holds only the elements its filter keeps (`HistoryFilter`,
 * `DebugFilter`), and those of an editor when one is given: the server
 * skips the others. Each element knows the editor whose session wrote it
 * (`editor_id`, 0 outside an editor).
 */

import { request } from './http'
import type { Action } from './generated/Action'
import type { ConsoleCleared } from './generated/ConsoleCleared'
import type { DebugEntry } from './generated/DebugEntry'
import type { DebugLevel } from './generated/DebugLevel'
import type { DebugSource } from './generated/DebugSource'
import type { Notification } from './generated/Notification'
import type { QueryEntry } from './generated/QueryEntry'

/**
 * Queries kept by the history.
 */
export interface HistoryFilter {
  /**
   * Actions kept; all of them when empty.
   */
  action: Action[]
}

/**
 * Lines kept by the debug log: a level kept and a source kept.
 */
export interface DebugFilter {
  /**
   * Levels kept; all of them when empty.
   */
  level: DebugLevel[]
  /**
   * Sources kept; all of them when empty.
   */
  source: DebugSource[]
}

/**
 * Path of a page: the elements of `editor` that `filter` keeps, before
 * `before`, `limit` at most.
 *
 * @param path Path of the route.
 * @param filter Values kept, per field of the elements.
 * @param editor Id of the editor whose elements are read; `null`, those of
 *     all the editors and of Prisme itself.
 * @param before Id of the oldest element already read; absent, the most
 *     recent elements.
 * @param limit Number of elements, 500 by default on the server's side.
 */
function pagePath(
  path: string,
  filter: HistoryFilter | DebugFilter,
  editor: number | null,
  before?: number,
  limit?: number,
): string {
  const parameters = new URLSearchParams()
  // One key per value kept: `level=error&level=warn`.
  for (const [field, values] of Object.entries(filter)) {
    for (const value of values) {
      parameters.append(field, value)
    }
  }
  if (editor !== null) {
    parameters.set('editor', String(editor))
  }
  if (before !== undefined) {
    parameters.set('before', String(before))
  }
  if (limit !== undefined) {
    parameters.set('limit', String(limit))
  }
  const query = parameters.toString()
  return query === '' ? path : `${path}?${query}`
}

/**
 * Reads a page of the history, in chronological order.
 *
 * @param filter Queries kept.
 * @param editor Id of the editor whose queries are read; `null`, all of
 *     them.
 * @param before Id of the oldest query already read; absent, the most
 *     recent queries.
 * @param limit Number of queries, 500 by default on the server's side.
 */
export function listHistory(
  filter: HistoryFilter,
  editor: number | null,
  before?: number,
  limit?: number,
): Promise<QueryEntry[]> {
  return request('GET', pagePath('/api/console/history', filter, editor, before, limit))
}

/**
 * Reads a page of the debug log, in chronological order.
 *
 * @param filter Lines kept.
 * @param editor Id of the editor whose lines are read; `null`, all of them.
 * @param before Id of the oldest line already read; absent, the most
 *     recent lines.
 * @param limit Number of lines, 500 by default on the server's side.
 */
export function listDebug(
  filter: DebugFilter,
  editor: number | null,
  before?: number,
  limit?: number,
): Promise<DebugEntry[]> {
  return request('GET', pagePath('/api/console/debug', filter, editor, before, limit))
}

/**
 * Path of a list to clear: the elements of `editor`, all of them when
 * `null`.
 *
 * @param path Path of the route.
 * @param editor Id of the editor.
 */
function clearPath(path: string, editor: number | null): string {
  return editor === null ? path : `${path}?editor=${editor}`
}

/**
 * Clears the user's history: the queries of an editor, or all of them.
 *
 * @param editor Id of the editor; `null`, the whole history.
 */
export function clearHistory(editor: number | null): Promise<void> {
  return request('DELETE', clearPath('/api/console/history', editor))
}

/**
 * Clears the debug log: the lines of an editor, or all of them.
 *
 * @param editor Id of the editor; `null`, the whole debug log.
 */
export function clearDebug(editor: number | null): Promise<void> {
  return request('DELETE', clearPath('/api/console/debug', editor))
}

/**
 * Adds a notification of the interface to the debug log. A failure is
 * ignored: the notification is shown anyway, and the request is not one of
 * the user's changes (the synchronisation state does not follow it).
 *
 * @param notification Level and text of the notification.
 */
export function logNotification(notification: Notification): void {
  fetch('/api/console/notifications', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(notification),
  }).catch(() => {
    // Server unreachable: the notification stays on screen only.
  })
}

/**
 * Receivers of the console's events.
 */
export interface ConsoleHandlers {
  /**
   * New query in the history.
   */
  onQuery: (entry: QueryEntry) => void
  /**
   * New line in the debug log.
   */
  onLine: (entry: DebugEntry) => void
  /**
   * History cleared, from this window or another one: the queries of an
   * editor, or all of them (`null`).
   */
  onHistoryCleared: (editorId: number | null) => void
  /**
   * Debug log cleared, from this window or another one: the lines of an
   * editor, or all of them (`null`).
   */
  onDebugCleared: (editorId: number | null) => void
  /**
   * Events were lost, or the stream reopened after a break: what was
   * received may be incomplete.
   */
  onGap: () => void
}

/**
 * Elements cleared, read from their event.
 *
 * @param event Event `history-cleared` or `debug-cleared`.
 */
function clearedOf(event: Event): ConsoleCleared {
  return JSON.parse((event as MessageEvent<string>).data) as ConsoleCleared
}

/**
 * Follows the console's events until the returned function is called. The
 * browser reopens the stream by itself after a break.
 *
 * @param handlers Receivers of the events.
 * @returns Stops following.
 */
export function followConsole(handlers: ConsoleHandlers): () => void {
  const source = new EventSource('/api/console/stream')
  let opened = false
  source.addEventListener('open', () => {
    // A reopening may have missed events.
    if (opened) {
      handlers.onGap()
    }
    opened = true
  })
  source.addEventListener('history', (event) => {
    handlers.onQuery(JSON.parse((event as MessageEvent<string>).data) as QueryEntry)
  })
  source.addEventListener('debug', (event) => {
    handlers.onLine(JSON.parse((event as MessageEvent<string>).data) as DebugEntry)
  })
  source.addEventListener('history-cleared', (event) => {
    handlers.onHistoryCleared(clearedOf(event).editor_id)
  })
  source.addEventListener('debug-cleared', (event) => {
    handlers.onDebugCleared(clearedOf(event).editor_id)
  })
  source.addEventListener('lagged', () => handlers.onGap())
  return () => source.close()
}

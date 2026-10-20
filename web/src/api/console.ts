/**
 * Console: history of the queries sent to the target servers.
 *
 * The history is read by pages, from the most recent (`listHistory`), then
 * followed as it grows through server-sent events (`followConsole`): one
 * `history` event per new query, `lagged` when the server dropped events for
 * a client too slow, which then reads the history again.
 */

import { request } from './http'
import type { QueryEntry } from './generated/QueryEntry'

/**
 * Reads a page of the history, in chronological order.
 *
 * @param before Id of the oldest query already read; absent, the most
 *     recent queries.
 * @param limit Number of queries, 500 by default on the server's side.
 */
export function listHistory(before?: number, limit?: number): Promise<QueryEntry[]> {
  const parameters = new URLSearchParams()
  if (before !== undefined) {
    parameters.set('before', String(before))
  }
  if (limit !== undefined) {
    parameters.set('limit', String(limit))
  }
  const query = parameters.toString()
  return request('GET', `/api/console/history${query === '' ? '' : `?${query}`}`)
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
   * Events were lost, or the stream reopened after a break: what was
   * received may be incomplete.
   */
  onGap: () => void
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
    // A reopening may have missed queries.
    if (opened) {
      handlers.onGap()
    }
    opened = true
  })
  source.addEventListener('history', (event) => {
    handlers.onQuery(JSON.parse((event as MessageEvent<string>).data) as QueryEntry)
  })
  source.addEventListener('lagged', () => handlers.onGap())
  return () => source.close()
}

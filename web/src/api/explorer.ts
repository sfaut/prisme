/**
 * Object explorer: catalog of a connection, received progressively.
 *
 * The server sends one JSON line per message (`CatalogEvent`): the
 * databases, then the tables database by database, then the columns database
 * by database. Each message is passed on as soon as it is received, without
 * waiting for the end of the stream.
 */

import { ApiError, UNREACHABLE, fetchApi, toApiError } from './http'
import type { CatalogEvent } from './generated/CatalogEvent'

/**
 * Reads the catalog of a connection and passes on each message as soon as it arrives.
 *
 * @param connectionId Id of the connection.
 * @param onEvent Called for each message received.
 * @throws {ApiError} If the connection to the target server fails.
 */
export async function streamCatalog(
  connectionId: number,
  onEvent: (event: CatalogEvent) => void,
): Promise<void> {
  const response = await fetchApi(`/api/connections/${connectionId}/catalog`)
  if (!response.ok || response.body === null) {
    throw await toApiError(response)
  }
  const reader = response.body.pipeThrough(new TextDecoderStream()).getReader()
  let pending = ''
  for (;;) {
    const { value, done } = await reader.read().catch((failure: unknown) => {
      // The server stopped while it was sending the catalog.
      throw failure instanceof TypeError ? new ApiError(0, UNREACHABLE) : failure
    })
    if (done) {
      break
    }
    pending += value
    const lines = pending.split('\n')
    // The last line can be incomplete: it waits for what follows.
    pending = lines.pop() ?? ''
    for (const line of lines) {
      if (line.trim() !== '') {
        onEvent(JSON.parse(line) as CatalogEvent)
      }
    }
  }
}

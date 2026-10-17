/**
 * Dialects the user writes in, and preview of the transpilation of a text
 * towards the dialect of the server (`crates/server/src/transpilation.rs`).
 */

import { request } from './http'
import type { DialectOption } from './generated/DialectOption'
import type { TranspileRequest } from './generated/TranspileRequest'
import type { TranspiledQuery } from './generated/TranspiledQuery'

/**
 * Dialects offered, with their versions.
 */
export function listDialects(): Promise<DialectOption[]> {
  return request('GET', '/api/dialects')
}

/**
 * Transpiles the queries of a text without running them: the one under the
 * cursor, or all of them without a cursor.
 *
 * @param body Text, dialect, connection and current database.
 * @throws {ApiError} If the dialect is unknown, or the server unreachable.
 */
export function previewTranspilation(body: TranspileRequest): Promise<TranspiledQuery[]> {
  return request('POST', '/api/transpile', body)
}

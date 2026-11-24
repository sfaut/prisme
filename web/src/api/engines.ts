/**
 * Engines of Prisme, read before any session (`GET /api/engines`): the
 * flavors each one serves and what it can do. The settings tell with them
 * the engines that each switch of the writes concerns.
 */

import { request } from './http'
import type { EngineInfo } from './generated/EngineInfo'
import type { Flavor } from './generated/Flavor'
import type { WriteKind } from './generated/WriteKind'

/**
 * Engines read, kept for the page: they change only with the executable. A
 * failed reading is forgotten, to be tried again.
 */
let read: Promise<EngineInfo[]> | null = null

/**
 * Reads the engines of Prisme, with their flavors and capabilities, once
 * for the page.
 */
export async function fetchEngines(): Promise<EngineInfo[]> {
  read ??= request<EngineInfo[]>('GET', '/api/engines', undefined, { sync: false }).catch(
    (failure: unknown) => {
      read = null
      throw failure
    },
  )
  return read
}

/**
 * Flavors whose engine knows a kind of write, in the order of the engines.
 *
 * @param engines Engines of Prisme.
 * @param kind Kind of write.
 */
export function flavorsOfWriteKind(engines: EngineInfo[], kind: WriteKind): Flavor[] {
  return engines.flatMap((engine) =>
    engine.capabilities.write_kinds.includes(kind) ? engine.flavors : [],
  )
}

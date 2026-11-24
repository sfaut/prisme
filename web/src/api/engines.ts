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
 * Reads the engines of Prisme, with their flavors and capabilities.
 */
export async function fetchEngines(): Promise<EngineInfo[]> {
  return request<EngineInfo[]>('GET', '/api/engines', undefined, { sync: false })
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

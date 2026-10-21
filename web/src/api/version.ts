/**
 * Version of Prisme: that of the executable, the engine, which the interface
 * shows beneath the name.
 */

import { request } from './http'
import type { AppVersion } from './generated/AppVersion'

/**
 * Reads the version of the executable.
 */
export function getVersion(): Promise<AppVersion> {
  return request('GET', '/api/version', undefined, { sync: false })
}

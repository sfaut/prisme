/**
 * Names of the server flavors, as the interface writes them.
 *
 * Shared by the messages of the errors and by the navigator, which names the
 * server of each source.
 */

import type { Flavor } from './generated/Flavor'

/**
 * Name of each flavor of server.
 */
export const FLAVOR_NAMES: Record<Flavor, string> = {
  mysql: 'MySQL',
  mariadb: 'MariaDB',
}

/**
 * Name and version of a server: `MariaDB 10.11.11-MariaDB`, `MySQL 8.4.11`.
 *
 * @param flavor Flavor of the server.
 * @param version Raw result of `SELECT VERSION()`, `null` if unknown.
 */
export function serverName(flavor: Flavor, version: string | null): string {
  return version === null ? FLAVOR_NAMES[flavor] : `${FLAVOR_NAMES[flavor]} ${version}`
}

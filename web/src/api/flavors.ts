/**
 * Registry of the flavors of source: what the interface knows of each one.
 *
 * Each flavor declares here its name and the syntax of the CTE that the
 * interface writes for it (also that of the `IN` clause of a selection). The
 * components read this registry and the capabilities of the source, and never
 * test a flavor themselves: a new flavor that forgets an entry is reported by
 * the compiler (`Record<Flavor, …>`). The colors of its badge come from the
 * theme (`ConnectionFlavorBadge.vue`).
 */

import type { Flavor } from './generated/Flavor'

/**
 * How the CTE of a result (`cte.ts`) is written for a flavor.
 */
export interface CteSyntax {
  /**
   * Are the rows of `VALUES` written `ROW (…)` (MySQL), rather than `(…)`?
   */
  rowConstructor: boolean
  /**
   * Is a date whose month or day is zero written as a string, which the
   * server reads where a typed literal (`DATE '2024-00-00'`) is refused?
   */
  zeroDatesAsStrings: boolean
  /**
   * Is a JSON value cast (`CAST('…' AS JSON)`), the flavor having a JSON type
   * of its own?
   */
  castsJson: boolean
  /**
   * First version of the flavor that reads its CTE (major, minor, patch),
   * `null` when every supported version does.
   */
  readSince: [number, number, number] | null
}

/**
 * What the interface knows of a flavor.
 */
export interface FlavorSpec {
  /**
   * Name of the product, as the interface writes it.
   */
  name: string
  /**
   * Syntax of the CTE of a result.
   */
  cte: CteSyntax
}

/**
 * Registry of the flavors.
 */
export const FLAVORS: Record<Flavor, FlavorSpec> = {
  mysql: {
    name: 'MySQL',
    cte: { rowConstructor: true, zeroDatesAsStrings: true, castsJson: true, readSince: [8, 0, 19] },
  },
  mariadb: {
    name: 'MariaDB',
    cte: { rowConstructor: false, zeroDatesAsStrings: false, castsJson: false, readSince: null },
  },
}

/**
 * Every flavor, in the order of the registry.
 */
export const ALL_FLAVORS = Object.keys(FLAVORS) as Flavor[]

/**
 * Name and version of a server: `MariaDB 10.11.11-MariaDB`, `MySQL 8.4.11`.
 *
 * @param flavor Flavor of the server.
 * @param version Raw result of `SELECT VERSION()`, `null` if unknown.
 */
export function serverName(flavor: Flavor, version: string | null): string {
  const name = FLAVORS[flavor].name
  return version === null ? name : `${name} ${version}`
}

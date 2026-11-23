/**
 * Names of the levels of namespaces, as the interface writes them.
 *
 * Each engine tells the levels of its namespaces (`Capabilities`): a
 * database for MySQL, a schema for SQLite, a catalog then a schema for
 * DuckDB. The navigator and the completion name a namespace by its level,
 * with the article and the agreement French asks for.
 */

import type { Capabilities } from './generated/Capabilities'
import type { NamespaceLevel } from './generated/NamespaceLevel'

/**
 * How the interface writes a level.
 */
export interface LevelNames {
  /**
   * Name, in lowercase: `base`, `schéma`.
   */
  name: string
  /**
   * Name after a demonstrative: `cette base`, `ce schéma`.
   */
  this: string
  /**
   * Current one of the session, capitalized: `Base courante`, `Schéma
   * courant`.
   */
  current: string
}

/**
 * Names of each level.
 */
export const LEVEL_NAMES: Record<NamespaceLevel, LevelNames> = {
  database: { name: 'base', this: 'cette base', current: 'Base courante' },
  schema: { name: 'schéma', this: 'ce schéma', current: 'Schéma courant' },
  catalog: { name: 'catalogue', this: 'ce catalogue', current: 'Catalogue courant' },
}

/**
 * Level of the namespaces that hold the objects, the innermost one; a
 * database while the capabilities are unknown.
 *
 * @param capabilities Capabilities of the source, `null` while unknown.
 */
export function innerLevel(capabilities: Capabilities | null | undefined): NamespaceLevel {
  const levels = capabilities?.namespace_levels ?? []
  return levels[levels.length - 1] ?? 'database'
}

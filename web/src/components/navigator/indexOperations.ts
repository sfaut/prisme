/**
 * Operations on an index, written from the object navigator (ticket #21): the
 * button of an index's line appends to the editor the statements that add an
 * index or act on this one, commented out. The user uncomments the one to
 * run, then fills in the names left as placeholders (`column_name`,
 * `new_index_name`).
 *
 * The block follows the flavor and the version of the server, as measured on
 * MySQL 8.0.11 and 9.3.0, MariaDB 10.5.29, 10.6.3 and 11.8.9:
 * - visibility: `ALTER INDEX i VISIBLE` / `INVISIBLE` in MySQL from 8.0;
 *   `ALTER INDEX i NOT IGNORED` / `IGNORED` in MariaDB from 10.6 (syntax
 *   error in 10.5; MariaDB knows no `INVISIBLE` index);
 * - renaming: `RENAME KEY i TO j`, in MySQL from 5.7, in MariaDB from 10.5.2
 *   (documentation of each engine, no older server measured);
 * - primary key: neither invisible (MySQL error 3522) nor ignored (MariaDB
 *   error 4174), and named `PRIMARY` for good: it is removed by
 *   `DROP PRIMARY KEY`.
 *
 * SQLite has no `ALTER TABLE` of the keys (`docs/SQL-COMPARISON.md`, section
 * 6): its block adds a unique index or an index by `CREATE [UNIQUE] INDEX`,
 * the schema on the index and the table unqualified, which it requires, then
 * rebuilds this one (`REINDEX`, under the "Maintenance" switch) and drops it
 * (`DROP INDEX`). The primary key of a table is never added afterwards; an
 * index of a constraint (`sqlite_autoindex_…`, a name SQLite reserves) is
 * never dropped; no index is renamed nor hidden.
 */

import type { Flavor } from '@/api/generated/Flavor'

import { quoteIdentifier } from './navigatorNodes'

/**
 * Version of a server, as `[major, minor, patch]`.
 */
type Version = [number, number, number]

/**
 * Server the operations are written for.
 */
export interface IndexServer {
  /**
   * Flavor of the server.
   */
  flavor: Flavor
  /**
   * Version of the server (`8.0.36`, `10.6.28-MariaDB-ubu2004`).
   */
  version: string
  /**
   * Is the `ANSI_QUOTES` mode active on the session? Names are then quoted
   * with double quotes.
   */
  ansiQuotes: boolean
}

/**
 * Index the operations act on.
 */
export interface IndexTarget {
  /**
   * Name of the database.
   */
  schema: string
  /**
   * Name of the table.
   */
  table: string
  /**
   * Name of the index, `PRIMARY` for the primary key.
   */
  name: string
}

/**
 * First versions that know the visibility of an index, then its renaming,
 * by flavor; `null` for a flavor that knows neither (SQLite).
 */
const VISIBILITY_SINCE: Record<Flavor, Version | null> = {
  mysql: [8, 0, 0],
  mariadb: [10, 6, 0],
  sqlite: null,
}
const RENAME_SINCE: Record<Flavor, Version | null> = {
  mysql: [5, 7, 0],
  mariadb: [10, 5, 2],
  sqlite: null,
}

/**
 * Keywords that make an index visible, then invisible, by flavor.
 */
const VISIBILITY_KEYWORDS: Record<Flavor, [string, string] | null> = {
  mysql: ['VISIBLE', 'INVISIBLE'],
  mariadb: ['NOT IGNORED', 'IGNORED'],
  sqlite: null,
}

/**
 * Does the server know an operation, from its first version (`null`: it
 * knows none)?
 *
 * @param server Server the operation is written for.
 * @param since First version of its flavor that knows the operation.
 */
function knows(server: IndexServer, since: Version | null): boolean {
  return since !== null && versionAtLeast(server.version, since)
}

/**
 * Name of the primary key in the catalog, whatever the flavor.
 */
const PRIMARY = 'PRIMARY'

/**
 * Prefix of the names that SQLite reserves: an index of such a name is that
 * of a `UNIQUE` or `PRIMARY KEY` constraint, which it never drops.
 */
const SQLITE_RESERVED = 'sqlite_'

/**
 * Statements of the operations of each flavor, before they are commented
 * out.
 */
const STATEMENTS: Record<Flavor, (target: IndexTarget, server: IndexServer) => string[]> = {
  mysql: alterTableOperations,
  mariadb: alterTableOperations,
  sqlite: createIndexOperations,
}

/**
 * Is a server version at least `minimum`? A version that cannot be read is
 * not: the statements that need it are left out.
 *
 * @param version Version of the server, as it gives it.
 * @param minimum First version that suits.
 */
export function versionAtLeast(version: string, minimum: Version): boolean {
  const parts = /^(\d+)\.(\d+)\.(\d+)/.exec(version)
  if (parts === null) {
    return false
  }
  const [major, minor, patch] = parts.slice(1).map(Number) as Version
  const order = major - minimum[0] || minor - minimum[1] || patch - minimum[2]
  return order >= 0
}

/**
 * Commented-out operations on an index, one statement per line, each ended
 * by the separator in force, in the form of the server's flavor.
 *
 * @param target Index the operations act on.
 * @param server Server the operations are written for.
 * @param delimiter Separator in force at the end of the editor's text.
 */
export function indexOperations(
  target: IndexTarget,
  server: IndexServer,
  delimiter: string,
): string {
  return STATEMENTS[server.flavor](target, server)
    .map((statement) => `-- ${statement}${delimiter}`)
    .join('\n')
}

/**
 * Operations of MySQL and MariaDB, by `ALTER TABLE`: adding a primary key, a
 * unique index, an index, then, for the primary key, its removal; for
 * another index, its visibility, its removal and its renaming, when the
 * server knows them.
 *
 * @param target Index the operations act on.
 * @param server Server the operations are written for.
 */
function alterTableOperations(target: IndexTarget, server: IndexServer): string[] {
  const quote = (name: string) => quoteIdentifier(name, server.ansiQuotes)
  const alter = `ALTER TABLE ${quote(target.schema)}.${quote(target.table)}`
  const index = quote(target.name)
  const statements = [
    `${alter} ADD PRIMARY KEY (column_name /* , ... */)`,
    `${alter} ADD UNIQUE KEY (column_name /* , ... */)`,
    `${alter} ADD KEY (column_name /* , ... */)`,
  ]
  if (target.name === PRIMARY) {
    statements.push(`${alter} DROP PRIMARY KEY`)
  } else {
    const keywords = VISIBILITY_KEYWORDS[server.flavor]
    if (keywords !== null && knows(server, VISIBILITY_SINCE[server.flavor])) {
      const [visible, invisible] = keywords
      statements.push(`${alter} ALTER INDEX ${index} ${visible}`)
      statements.push(`${alter} ALTER INDEX ${index} ${invisible}`)
    }
    statements.push(`${alter} DROP KEY ${index}`)
    if (knows(server, RENAME_SINCE[server.flavor])) {
      statements.push(`${alter} RENAME KEY ${index} TO new_index_name`)
    }
  }
  return statements
}

/**
 * Operations of SQLite: adding a unique index, an index, then, for an index
 * other than the primary key, its rebuilding and, unless it is that of a
 * constraint, its removal.
 *
 * @param target Index the operations act on.
 * @param server Server the operations are written for.
 */
function createIndexOperations(target: IndexTarget, server: IndexServer): string[] {
  const quote = (name: string) => quoteIdentifier(name, server.ansiQuotes)
  const schema = quote(target.schema)
  const on = `ON ${quote(target.table)} (column_name /* , ... */)`
  const statements = [
    `CREATE UNIQUE INDEX ${schema}.new_index_name ${on}`,
    `CREATE INDEX ${schema}.new_index_name ${on}`,
  ]
  if (target.name !== PRIMARY) {
    const index = `${schema}.${quote(target.name)}`
    statements.push(`REINDEX ${index}`)
    if (!target.name.toLowerCase().startsWith(SQLITE_RESERVED)) {
      statements.push(`DROP INDEX ${index}`)
    }
  }
  return statements
}

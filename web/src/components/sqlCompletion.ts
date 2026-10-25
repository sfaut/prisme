/**
 * Completion of the SQL editor from the connection's catalog.
 *
 * The catalog loaded for the navigator becomes the namespace of
 * `@codemirror/lang-sql`: databases, then their tables, views, functions and
 * procedures, then the columns of the tables. Completion then offers the
 * names of the current database without a prefix, and the columns after
 * `table.` or `alias.`. Each option shows its nature (database, table, view,
 * function, procedure) or, for a column, its type.
 *
 * A name that is not a plain identifier (space, dash, leading digit...) or
 * that is a reserved word of the server is inserted quoted: double quotes if
 * the `ANSI_QUOTES` mode is active on the session, backticks otherwise.
 */

import type { Completion } from '@codemirror/autocomplete'
import type { SQLNamespace } from '@codemirror/lang-sql'

import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { CatalogSchema, CatalogTable } from '@/stores/catalog'

import { displayedType, quoteIdentifier, upperType } from './navigatorNodes'

/**
 * Plain identifier, written without quotes: letters, digits, `_`, `$` and
 * the characters from U+00A1, not starting with a digit. MySQL and MariaDB
 * accept a leading digit, but read `123`, `1e5` or `0x1F` as numbers: such
 * names are quoted, as are the other names starting with a digit, which
 * quotes keep valid. `lang-sql` has its own rule (`^[a-z_][a-z_\d]*$`),
 * which it only applies to the names it receives as plain strings: the names
 * given here as `Completion` objects are quoted by Prisme.
 */
const PLAIN_IDENTIFIER = /^[a-z_$\xa1-￿][\w$\xa1-￿]*$/i

/**
 * How the names of the completion are quoted.
 */
interface Quoting {
  /**
   * Is the `ANSI_QUOTES` mode active on the session?
   */
  ansiQuotes: boolean
  /**
   * Reserved words of the server, in capitals. A name that is one is quoted,
   * even after a dot, where the server would accept it bare: quotes keep it
   * valid everywhere.
   */
  reserved: ReadonlySet<string>
}

/**
 * Completion option of a name, quoted on insertion if it is not a plain
 * identifier or if it is a reserved word.
 *
 * Workaround: `lang-sql` does not quote the names given as `Completion`
 * objects (`docs/DEVELOPER.md`, section "Workarounds").
 *
 * @param name Offered name.
 * @param quoting How the names are quoted.
 * @param rest Nature and detail of the option.
 */
function nameCompletion(
  name: string,
  quoting: Quoting,
  rest: Omit<Completion, 'label'>,
): Completion {
  const completion: Completion = { label: name, ...rest }
  if (!PLAIN_IDENTIFIER.test(name) || quoting.reserved.has(name.toUpperCase())) {
    completion.apply = quoteIdentifier(name, quoting.ansiQuotes)
  }
  return completion
}

/**
 * Namespace of the completion, built from the catalog, even incomplete: a
 * part not received yet is simply not offered.
 *
 * @param schemas Databases of the catalog.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 * @param reserved
 *     Reserved words of the server, in capitals; none while they are not
 *     received.
 */
export function completionSchema(
  schemas: CatalogSchema[],
  ansiQuotes = false,
  reserved: ReadonlySet<string> = new Set(),
): SQLNamespace {
  const quoting: Quoting = { ansiQuotes, reserved }
  const namespace: Record<string, SQLNamespace> = {}
  for (const schema of schemas) {
    const children: Record<string, SQLNamespace> = {}
    for (const table of schema.tables ?? []) {
      children[table.name] = tableNamespace(table, quoting)
    }
    for (const routine of schema.routines ?? []) {
      children[routine.name] = routineNamespace(routine, quoting)
    }
    namespace[schema.name] = {
      self: nameCompletion(schema.name, quoting, { type: 'namespace', detail: 'base' }),
      children,
    }
  }
  return namespace
}

/**
 * Namespace of a stored function or procedure: its name only.
 *
 * @param routine Routine of the catalog.
 * @param quoting How the names are quoted.
 */
function routineNamespace(routine: RoutineInfo, quoting: Quoting): SQLNamespace {
  return {
    self: nameCompletion(routine.name, quoting, {
      type: 'function',
      detail: routine.kind === 'function' ? 'fonction' : 'procédure',
    }),
    children: [],
  }
}

/**
 * Namespace of a table or a view: the table itself, then its columns with their type.
 *
 * @param table Table or view of the catalog.
 * @param quoting How the names are quoted.
 */
function tableNamespace(table: CatalogTable, quoting: Quoting): SQLNamespace {
  return {
    self: nameCompletion(table.name, quoting, {
      type: 'type',
      detail: table.kind === 'table' ? 'table' : 'vue',
    }),
    children: (table.details?.columns ?? []).map((column) =>
      nameCompletion(column.name, quoting, {
        type: 'property',
        detail: upperType(displayedType(column.column_type)),
      }),
    ),
  }
}

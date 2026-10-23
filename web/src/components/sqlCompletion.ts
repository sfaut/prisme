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
 * A name that is not a plain identifier (space, dash...) is inserted quoted:
 * double quotes if the `ANSI_QUOTES` mode is active on the session, backticks
 * otherwise.
 */

import type { Completion } from '@codemirror/autocomplete'
import type { SQLNamespace } from '@codemirror/lang-sql'

import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { CatalogSchema, CatalogTable } from '@/stores/catalog'

import { displayedType, quoteIdentifier, upperType } from './navigatorNodes'

/**
 * Plain identifier, written without quotes (same rule as `lang-sql`).
 */
const PLAIN_IDENTIFIER = /^[\w$\xa1-\uffff][\w$\d\xa1-\uffff]*$/

/**
 * Completion option of a name, quoted on insertion if it is not a plain
 * identifier.
 *
 * @param name Offered name.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 * @param rest Nature and detail of the option.
 */
function nameCompletion(
  name: string,
  ansiQuotes: boolean,
  rest: Omit<Completion, 'label'>,
): Completion {
  const completion: Completion = { label: name, ...rest }
  if (!PLAIN_IDENTIFIER.test(name)) {
    completion.apply = quoteIdentifier(name, ansiQuotes)
  }
  return completion
}

/**
 * Namespace of the completion, built from the catalog, even incomplete: a
 * part not received yet is simply not offered.
 *
 * @param schemas Databases of the catalog.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
export function completionSchema(schemas: CatalogSchema[], ansiQuotes = false): SQLNamespace {
  const namespace: Record<string, SQLNamespace> = {}
  for (const schema of schemas) {
    const children: Record<string, SQLNamespace> = {}
    for (const table of schema.tables ?? []) {
      children[table.name] = tableNamespace(table, ansiQuotes)
    }
    for (const routine of schema.routines ?? []) {
      children[routine.name] = routineNamespace(routine, ansiQuotes)
    }
    namespace[schema.name] = {
      self: nameCompletion(schema.name, ansiQuotes, { type: 'namespace', detail: 'base' }),
      children,
    }
  }
  return namespace
}

/**
 * Namespace of a stored function or procedure: its name only.
 *
 * @param routine Routine of the catalog.
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
function routineNamespace(routine: RoutineInfo, ansiQuotes: boolean): SQLNamespace {
  return {
    self: nameCompletion(routine.name, ansiQuotes, {
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
 * @param ansiQuotes Is the `ANSI_QUOTES` mode active on the session?
 */
function tableNamespace(table: CatalogTable, ansiQuotes: boolean): SQLNamespace {
  return {
    self: nameCompletion(table.name, ansiQuotes, {
      type: 'type',
      detail: table.kind === 'table' ? 'table' : 'vue',
    }),
    children: (table.details?.columns ?? []).map((column) =>
      nameCompletion(column.name, ansiQuotes, {
        type: 'property',
        detail: upperType(displayedType(column.column_type)),
      }),
    ),
  }
}

/**
 * Complétion de l'éditeur SQL à partir du catalogue de la connexion.
 *
 * Le catalogue chargé pour l'explorateur devient l'espace de noms de
 * `@codemirror/lang-sql` : bases, puis leurs tables, vues, fonctions et
 * procédures, puis les colonnes des tables. La complétion propose alors les
 * noms de la base en cours sans préfixe, et les colonnes après `table.` ou
 * `alias.`. Chaque proposition affiche sa nature (base, table, vue,
 * fonction, procédure) ou, pour une colonne, son type.
 *
 * Un nom qui n'est pas un identifiant simple (espace, tiret...) s'insère
 * délimité : guillemets doubles si le mode `ANSI_QUOTES` est actif sur la
 * session, accents graves sinon.
 */

import type { Completion } from '@codemirror/autocomplete'
import type { SQLNamespace } from '@codemirror/lang-sql'

import type { RoutineInfo } from '@/api/generated/RoutineInfo'
import type { CatalogSchema, CatalogTable } from '@/stores/catalog'

import { displayedType, quoteIdentifier, upperType } from './explorerNodes'

/**
 * Identifiant simple, qui s'écrit sans délimiteur (même règle que `lang-sql`).
 */
const PLAIN_IDENTIFIER = /^[\w$\xa1-\uffff][\w$\d\xa1-\uffff]*$/

/**
 * Proposition de complétion d'un nom, délimité à l'insertion s'il n'est pas
 * un identifiant simple.
 *
 * @param name Nom proposé.
 * @param ansiQuotes Le mode `ANSI_QUOTES` est-il actif sur la session ?
 * @param rest Nature et détail de la proposition.
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
 * Espace de noms de la complétion, construit à partir du catalogue, même
 * incomplet : une partie pas encore reçue n'est simplement pas proposée.
 *
 * @param schemas Bases du catalogue.
 * @param ansiQuotes Le mode `ANSI_QUOTES` est-il actif sur la session ?
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
 * Espace de noms d'une fonction ou d'une procédure stockée : son seul nom.
 *
 * @param routine Routine du catalogue.
 * @param ansiQuotes Le mode `ANSI_QUOTES` est-il actif sur la session ?
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
 * Espace de noms d'une table ou d'une vue : la table elle-même, puis ses colonnes avec leur type.
 *
 * @param table Table ou vue du catalogue.
 * @param ansiQuotes Le mode `ANSI_QUOTES` est-il actif sur la session ?
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

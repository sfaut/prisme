/**
 * Result of the test of a database held in files, as the "Vérification"
 * step of a creation shows it: its tone, its sentence and the fixes it
 * offers, from its input (what is asked: an existing file, a new one,
 * memory) and the state the server tested.
 *
 * The test tells the state of each database, the main one included, even
 * when the main one cannot be opened (`docs/TODO-SQLITE.md`, decision 7): a
 * missing database does not prevent saving, a session opening without it,
 * except the main one, without which no session opens.
 */
import type { DatabaseState } from '@/api/generated/DatabaseState'
import { connectMessage } from '@/api/messages'
import { formatBytes } from '@/api/querySession'

import { type DatabaseInput, MAIN_SCHEMA } from './connectionForm'

/**
 * Fix offered for a database: create its file at saving, open the file
 * that exists as it is, or remove the database from the connection.
 */
export type DatabaseFix = 'create' | 'open' | 'remove'

/**
 * Result of the test of a database.
 */
export interface DatabaseCheck {
  /**
   * Tone, that of the banners (`UiAlert`).
   */
  tone: 'info' | 'success' | 'warning' | 'error'
  /**
   * Sentence that says the state.
   */
  text: string
  /**
   * Fixes offered, in the order shown.
   */
  fixes: DatabaseFix[]
}

/**
 * State tested of the database `schema`, among `states`: SQLite tells its
 * schemas apart without their case.
 *
 * @param states States of the databases, as the test gave them.
 * @param schema Schema of the database.
 */
export function stateOf(states: DatabaseState[], schema: string): DatabaseState | undefined {
  const name = schema.trim().toLowerCase()
  return states.find((state) => state.schema.toLowerCase() === name)
}

/**
 * Sentence that adds to the message of the test the databases to look at
 * again, those whose test warns or fails; `null` if none.
 *
 * @param inputs Inputs of the databases.
 * @param states Their states, as the test gave them.
 */
export function reviewText(inputs: DatabaseInput[], states: DatabaseState[]): string | null {
  const count = inputs.filter((input) => {
    const { tone } = checkOf(input, stateOf(states, input.schema))
    return tone === 'warning' || tone === 'error'
  }).length
  if (count === 0) {
    return null
  }
  return count === 1 ? '1 base à revoir, ci-dessous.' : `${count} bases à revoir, ci-dessous.`
}

/**
 * Result of the test of a database.
 *
 * @param input Input of the database.
 * @param state Its state, as the test gave it; absent, it was not tested.
 */
export function checkOf(input: DatabaseInput, state: DatabaseState | undefined): DatabaseCheck {
  const main = input.schema.trim().toLowerCase() === MAIN_SCHEMA
  const without = main
    ? 'Sans elle, aucune session ne s’ouvre.'
    : 'Les sessions s’ouvriront sans elle.'
  const removable: DatabaseFix[] = main ? [] : ['remove']
  if (state === undefined) {
    return { tone: 'info', text: 'Non testée.', fixes: [] }
  }
  if (state.status === 'open') {
    if (input.origin === 'memory') {
      return { tone: 'info', text: 'En mémoire : vide à chaque session.', fixes: [] }
    }
    if (input.origin === 'new') {
      return {
        tone: 'error',
        text: 'Le fichier existe déjà : l’enregistrement sera refusé.',
        fixes: ['open', ...removable],
      }
    }
    const read_only = state.read_only ? ', en lecture seule' : ''
    return {
      tone: 'success',
      text: `Ouverte : ${formatBytes(state.bytes)}, journal ${state.journal_mode}${read_only}.`,
      fixes: [],
    }
  }
  const { failure } = state
  if (failure.reason === 'file_not_found') {
    if (input.origin === 'new') {
      return { tone: 'info', text: 'Créée vide à l’enregistrement.', fixes: [] }
    }
    const creatable: DatabaseFix[] = state.location.scheme === 'file' ? ['create'] : []
    return {
      tone: main ? 'error' : 'warning',
      text: `Introuvable. ${without}`,
      fixes: [...creatable, ...removable],
    }
  }
  return {
    tone: main ? 'error' : 'warning',
    text: `${connectMessage(failure)}\n${without}`,
    fixes: removable,
  }
}

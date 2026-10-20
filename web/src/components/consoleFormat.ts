/**
 * Formats of the console: for the history, name of the actions, time,
 * durations and the message of a query, its SQL followed by its outcome
 * (« SELECT * FROM video — 120 lignes »); for the debug log, name and tone
 * of the levels and name of the sources.
 */

import { formatDuration, formatInstant } from '@/api/querySession'
import type { Action } from '@/api/generated/Action'
import type { DebugLevel } from '@/api/generated/DebugLevel'
import type { DebugSource } from '@/api/generated/DebugSource'
import type { QueryEntry } from '@/api/generated/QueryEntry'

/**
 * Name of each action, as the console shows it.
 */
export const ACTION_LABELS: Record<Action, string> = {
  session: 'Session',
  run: 'Exécuter',
  run_all: 'Tout exécuter',
  refresh: 'Rafraîchir',
  use: 'Base courante',
  cancel: 'Interruption',
  explorer: 'Explorateur',
  probe: 'Test de connexion',
}

/**
 * Name of each level of the debug log.
 */
export const LEVEL_LABELS: Record<DebugLevel, string> = {
  error: 'Erreur',
  warn: 'Avertissement',
  info: 'Info',
  debug: 'Debug',
  trace: 'Trace',
}

/**
 * Color class of each level: failures and warnings stand out.
 */
export const LEVEL_TONES: Record<DebugLevel, string> = {
  error: 'text-error',
  warn: 'text-warning',
  info: 'text-fg',
  debug: 'text-fg-muted',
  trace: 'text-fg-dimmed',
}

/**
 * Name of each source of the debug log.
 */
export const SOURCE_LABELS: Record<DebugSource, string> = {
  server: 'Serveur',
  interface: 'Interface',
}

/**
 * Time of an instant, to the second, in local time: "14:32:05".
 *
 * @param instant Instant in ISO 8601 format.
 */
export function timeOf(instant: string): string {
  return new Date(instant).toLocaleTimeString('fr-FR')
}

/**
 * Date and time of an instant, for the tooltip of the time.
 *
 * @param instant Instant in ISO 8601 format.
 */
export function dateTimeOf(instant: string): string {
  return formatInstant(Date.parse(instant))
}

/**
 * Duration given in microseconds, in the unit that suits it.
 *
 * @param microseconds Duration.
 */
export function formatMicros(microseconds: number): string {
  return formatDuration(microseconds / 1000)
}

/**
 * Transfer time of a query, "—" when not measured.
 *
 * @param entry Query of the history.
 */
export function transferOf(entry: QueryEntry): string {
  return entry.transfer_us === 0 ? '—' : formatMicros(entry.transfer_us)
}

/**
 * Text of a query on a single line.
 *
 * @param sql Text of the query.
 */
export function oneLine(sql: string): string {
  return sql.replace(/\s+/g, ' ').trim()
}

/**
 * Number of rows, in French: "1 ligne", "1 200 lignes".
 *
 * @param count Number of rows.
 * @param qualifier Word after "ligne", agreed in number ("modifiée").
 */
function rows(count: number, qualifier = ''): string {
  const plural = count > 1 ? 's' : ''
  const word = qualifier === '' ? '' : ` ${qualifier}${plural}`
  return `${count.toLocaleString('fr-FR')} ligne${plural}${word}`
}

/**
 * Outcome of a query: its failure, its number of rows read or changed, or
 * "OK".
 *
 * @param entry Query of the history.
 */
export function outcomeOf(entry: QueryEntry): string {
  if (entry.error !== '') {
    return `erreur : ${entry.error}`
  }
  if (entry.returns_rows) {
    return `${rows(entry.row_count)}${entry.truncated ? ' (résultat tronqué)' : ''}`
  }
  if (entry.affected_rows > 0) {
    return rows(entry.affected_rows, 'modifiée')
  }
  return 'OK'
}

/**
 * Message of a query: its SQL on a single line, then its outcome.
 *
 * @param entry Query of the history.
 */
export function messageOf(entry: QueryEntry): string {
  return `${oneLine(entry.sql)} — ${outcomeOf(entry)}`
}

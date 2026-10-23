/**
 * Formats of the console: for the history, name of the actions, time,
 * durations, number of rows and the message of a query, its SQL followed by
 * its failure if any (« SELECT * FROM t — erreur : … »); for the debug log,
 * name and tone of the levels and name of the sources.
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
  run_all: 'Exécuter tout',
  run_exact: 'Exécuter précisément',
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
 * Number of rows of a query: those read, preceded by "+" when the result
 * was truncated ("+1 000"), or those changed by a write; "—" without either.
 *
 * @param entry Query of the history.
 */
export function rowsOf(entry: QueryEntry): string {
  if (entry.returns_rows) {
    return `${entry.truncated ? '+' : ''}${entry.row_count.toLocaleString('fr-FR')}`
  }
  return entry.affected_rows > 0 ? entry.affected_rows.toLocaleString('fr-FR') : '—'
}

/**
 * Tooltip of the number of rows: what it counts.
 *
 * @param entry Query of the history.
 */
export function rowsTitleOf(entry: QueryEntry): string | undefined {
  if (entry.returns_rows) {
    return entry.truncated ? 'Lignes lues, résultat tronqué' : 'Lignes lues'
  }
  return entry.affected_rows > 0 ? 'Lignes modifiées' : undefined
}

/**
 * Failure of a query, after its SQL in the message; empty on success.
 *
 * @param entry Query of the history.
 */
export function failureOf(entry: QueryEntry): string {
  return entry.error === '' ? '' : `— erreur : ${entry.error}`
}

/**
 * Message of a query: its SQL on a single line, then its failure if any.
 *
 * @param entry Query of the history.
 */
export function messageOf(entry: QueryEntry): string {
  return `${oneLine(entry.sql)} ${failureOf(entry)}`.trim()
}

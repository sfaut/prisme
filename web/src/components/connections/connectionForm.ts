/**
 * Input of a MySQL / MariaDB connection in the connections manager: empty
 * input, input of a saved connection, comparison with it (a changed input is
 * a draft), input of a copy, and the texts that describe a connection (where
 * it points, its last connection).
 *
 * The interface does not know the saved password: the input of a saved
 * connection has an empty password, which the server reads as "unchanged",
 * and as "that of the copied connection" for a copy.
 */

import type { ConnectionSummary } from '@/api/generated/ConnectionSummary'
import type { NewConnection } from '@/api/generated/NewConnection'

/**
 * Input of a connection, every field filled.
 */
export type ConnectionForm = Required<NewConnection>

/**
 * Maximum lengths of the fields, in characters, as the server checks them
 * (`crates/server/src/connections.rs`).
 */
export const LENGTHS = {
  name: 100,
  host: 253,
  username: 80,
  databaseName: 64,
  initSql: 4000,
} as const

/**
 * Longest keep-alive delay of a session, in minutes (24 hours).
 */
export const SESSION_TIMEOUT_MAX = 1440

/**
 * Empty input, with MySQL's default port.
 */
export function emptyForm(): ConnectionForm {
  return {
    name: '',
    host: '127.0.0.1',
    port: 3306,
    username: '',
    password: '',
    database_name: '',
    init_sql: '',
    session_timeout_minutes: 5,
    show_system_schemas: true,
  }
}

/**
 * Input of a saved connection, password empty.
 *
 * @param connection Saved connection.
 */
export function formOf(connection: ConnectionSummary): ConnectionForm {
  const {
    name,
    host,
    port,
    username,
    database_name,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
  } = connection
  return {
    name,
    host,
    port,
    username,
    password: '',
    database_name,
    init_sql,
    session_timeout_minutes,
    show_system_schemas,
  }
}

/**
 * Does an input differ from the saved connection? A typed password is a
 * change.
 *
 * @param form Current input.
 * @param connection Saved connection.
 */
export function isChanged(form: ConnectionForm, connection: ConnectionSummary): boolean {
  const saved = formOf(connection)
  return (Object.keys(saved) as (keyof ConnectionForm)[]).some((key) => form[key] !== saved[key])
}

/**
 * Input of a copy: the same settings, under a name of its own. The password
 * typed in the input goes with it; empty, the server takes that of the
 * copied connection.
 *
 * @param form Input of the copied connection.
 */
export function copyForm(form: ConnectionForm): ConnectionForm {
  return { ...form, name: `${form.name}-copie`.slice(0, LENGTHS.name) }
}

/**
 * Where a connection points: `host:port`.
 *
 * @param form Input or saved connection.
 */
export function placeOf(form: Pick<NewConnection, 'host' | 'port'>): string {
  return `${form.host}:${form.port}`
}

/**
 * Units of the relative dates, from the largest, with their length in seconds.
 */
const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 86_400],
  ['month', 30 * 86_400],
  ['day', 86_400],
  ['hour', 3_600],
  ['minute', 60],
]

/**
 * Relative date, in French: "il y a 5 minutes", "hier", "à l’instant".
 *
 * @param iso Date, in ISO 8601 format.
 * @param now Current time, in milliseconds since the epoch.
 */
export function sinceText(iso: string, now: number = Date.now()): string {
  const seconds = Math.round((Date.parse(iso) - now) / 1000)
  const format = new Intl.RelativeTimeFormat('fr', { numeric: 'auto' })
  for (const [unit, length] of UNITS) {
    if (Math.abs(seconds) >= length) {
      return format.format(Math.round(seconds / length), unit)
    }
  }
  return 'à l’instant'
}

/**
 * State of the last connection: "Connectée il y a 5 minutes", or "Jamais
 * connectée".
 *
 * @param connection Saved connection.
 * @param now Current time, in milliseconds since the epoch.
 */
export function connectedText(connection: ConnectionSummary, now: number = Date.now()): string {
  return connection.connected_at === null
    ? 'Jamais connectée'
    : `Connectée ${sinceText(connection.connected_at, now)}`
}

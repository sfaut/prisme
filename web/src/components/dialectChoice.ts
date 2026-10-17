/**
 * Choice of the dialect the user writes in, as the server stores it:
 * `source` for the native SQL of the data source, or the identifier of a
 * dialect followed by its version (`mysql 8.0.19`). The preferred dialect of
 * the profile applies to the editors that do not choose their own (empty
 * string).
 */

import type { DialectOption } from '@/api/generated/DialectOption'
import type { Flavor } from '@/api/generated/Flavor'

/**
 * Native SQL of the data source: the queries run as written.
 */
export const SOURCE = 'source'

/**
 * Label of the native SQL of the data source.
 */
export const SOURCE_LABEL = 'Selon source de données'

/**
 * Dialect that applies to an editor: its own, otherwise the preferred
 * dialect of the profile, otherwise the native SQL of the data source.
 *
 * @param editorDialect Dialect of the editor, empty string for the profile's.
 * @param preferred Preferred dialect of the profile, absent before it is read.
 */
export function effectiveDialect(editorDialect: string, preferred: string | undefined): string {
  return editorDialect || preferred || SOURCE
}

/**
 * Dialect and version of a choice, `null` for the native SQL of the source.
 *
 * @param choice Stored choice.
 */
export function parseChoice(choice: string): { id: Flavor; version: string } | null {
  const [id, version] = choice.split(' ')
  if ((id === 'mysql' || id === 'mariadb') && version) {
    return { id, version }
  }
  return null
}

/**
 * Names of the dialects.
 */
const LABELS: Record<Flavor, string> = { mysql: 'MySQL', mariadb: 'MariaDB' }

/**
 * Name of a choice, for the interface: "Selon source de données", "MySQL 8.0.19".
 *
 * @param choice Stored choice; also the dialect of a server (`mariadb 11.3.2`).
 */
export function choiceLabel(choice: string): string {
  const parsed = parseChoice(choice)
  return parsed === null ? SOURCE_LABEL : `${LABELS[parsed.id]} ${parsed.version}`
}

/**
 * Choices offered in a list: the native SQL of the source, then each version
 * of each dialect, from the oldest.
 *
 * @param options Dialects offered.
 */
export function choiceItems(options: DialectOption[]): { label: string; value: string }[] {
  return [
    { label: SOURCE_LABEL, value: SOURCE },
    ...options.flatMap((option) =>
      option.versions.map((version) => ({
        label: `${option.label} ${version}`,
        value: `${option.id} ${version}`,
      })),
    ),
  ]
}

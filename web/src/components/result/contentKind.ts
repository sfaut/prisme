/**
 * Detection of the content of text columns: JSON, XML or URL.
 *
 * A `TEXT` (or `VARCHAR`) column can hold JSON or XML documents, or web
 * addresses, which the server does not type (MariaDB's JSON also arrives as
 * `LONGTEXT`). The grid header then shows the icon of the content instead of
 * that of text; the addresses become links. Detection reads a sample of the
 * first non-empty values: all must be of the same format, otherwise the
 * column stays text.
 */

/**
 * Format recognized in a text column.
 */
export type ContentKind = 'json' | 'xml' | 'url'

/**
 * Web address: `http://` or `https://`, then no space. Only these two
 * protocols become links (never `javascript:` nor `data:`).
 */
const URL_PATTERN = /^https?:\/\/[^\s]+$/i

/**
 * Is the text a web address, to make clickable?
 *
 * @param value Text of the cell, `null` for NULL.
 */
export function isWebAddress(value: string | null): value is string {
  return value !== null && URL_PATTERN.test(value)
}

/**
 * Number of non-empty values read to decide.
 */
const SAMPLE_SIZE = 50

/**
 * Maximum number of rows scanned to gather the sample.
 */
const SCAN_LIMIT = 500

/**
 * Format of the values of the column `column`, `null` if it is neither JSON,
 * nor XML, nor web addresses (or if no value is there yet).
 *
 * @param rows Rows of the result.
 * @param column Index of the column.
 */
export function detectContent(rows: (string | null)[][], column: number): ContentKind | null {
  const sample: string[] = []
  for (let index = 0; index < rows.length && index < SCAN_LIMIT; index++) {
    const value = rows[index]?.[column]?.trim()
    if (value) {
      sample.push(value)
      if (sample.length === SAMPLE_SIZE) {
        break
      }
    }
  }
  if (sample.length === 0) {
    return null
  }
  if (sample.every(isJsonDocument)) {
    return 'json'
  }
  if (sample.every(isXmlDocument)) {
    return 'xml'
  }
  if (sample.every(isWebAddress)) {
    return 'url'
  }
  return null
}

/**
 * Is the text a JSON object or array? A lone number or string, valid JSON,
 * does not make a column a JSON column.
 *
 * @param value Text, without surrounding spaces.
 */
function isJsonDocument(value: string): boolean {
  const first = value[0]
  if (first !== '{' && first !== '[') {
    return false
  }
  try {
    JSON.parse(value)
    return true
  } catch {
    return false
  }
}

/**
 * Is the text a well-formed XML document?
 *
 * @param value Text, without surrounding spaces.
 */
function isXmlDocument(value: string): boolean {
  if (!value.startsWith('<') || !value.endsWith('>')) {
    return false
  }
  const document = new DOMParser().parseFromString(value, 'application/xml')
  return document.getElementsByTagName('parsererror').length === 0
}

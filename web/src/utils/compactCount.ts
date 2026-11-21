/**
 * Short form of a count, for a place where an order of magnitude is enough
 * (row count of a table in the object navigator).
 *
 * Below 1,000 the count is exact; above, French compact notation with one
 * decimal at most: `345`, `1,3 k`, `4,5 M`, `1,2 Md`. The space before the
 * unit is non-breaking, so the count never wraps.
 */

/**
 * Formatter of the counts, created once.
 */
const COMPACT = new Intl.NumberFormat('fr-FR', { notation: 'compact', maximumFractionDigits: 1 })

/**
 * Writes a count in its short form.
 *
 * @param count Count, a non-negative integer.
 */
export function compactCount(count: number): string {
  return COMPACT.format(count)
}

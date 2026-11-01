/**
 * Configuration of the Pivoteur for a result tab: the columns of the result placed in the zones
 * Lignes, Colonnes and Mesures (`PivotZones.vue`), and the labels of their elements.
 *
 * In this first slice, a level is a column of the result, by its index, and a measure is
 * `COUNT(*)`, `COUNT(col)`, `COUNT(DISTINCT col)` or `SUM(col)`; the expressions come with the
 * parser. A column is a
 * level at most once, on one of the two axes: placing it again moves it. A measure can be placed
 * several times. Each change gives a new configuration, which the panel computes.
 */

import type { TypeFamily } from '@/api/generated/TypeFamily'

import type { PivotMeasure, PivotRequest } from './pivotEngine'

/**
 * Zone of the panel.
 */
export type PivotZone = 'rows' | 'columns' | 'measures'

/**
 * Place of an element in a zone.
 */
export interface ZonePlace {
  /**
   * Zone.
   */
  zone: PivotZone
  /**
   * Position in the zone, from 0.
   */
  index: number
}

/**
 * Configuration without any element.
 */
export function emptyConfig(): PivotRequest {
  return { rows: [], columns: [], measures: [] }
}

/**
 * Is the configuration without any element?
 *
 * @param config Configuration.
 */
export function isEmptyConfig(config: PivotRequest): boolean {
  return config.rows.length === 0 && config.columns.length === 0 && config.measures.length === 0
}

/**
 * Can a column of this family be summed? Integers and decimals exactly, floats as `DOUBLE`.
 *
 * @param family
 *     Family of the type of the column; unknown for a result saved before its columns were
 *     typed.
 */
export function summable(family: TypeFamily | undefined): boolean {
  return family === 'integer' || family === 'decimal' || family === 'float'
}

/**
 * Measure of a column dropped into the measures: `SUM` for a number, `COUNT` otherwise (F-15).
 *
 * @param column Index of the column.
 * @param family Family of its type.
 */
export function defaultMeasure(column: number, family: TypeFamily | undefined): PivotMeasure {
  return { aggregate: summable(family) ? 'sum' : 'count', column }
}

/**
 * Label of a measure, as its expression: `SUM(montant)`, `COUNT(ville)`,
 * `COUNT(DISTINCT ville)`, `COUNT(*)`.
 *
 * @param measure Measure.
 * @param names Names of the columns of the result.
 */
export function measureLabel(measure: PivotMeasure, names: string[]): string {
  const name = names[measure.column] ?? '?'
  switch (measure.aggregate) {
    case 'count_star':
      return 'COUNT(*)'
    case 'count':
      return `COUNT(${name})`
    case 'count_distinct':
      return `COUNT(DISTINCT ${name})`
    case 'sum':
      return `SUM(${name})`
  }
}

/**
 * Column of an element of a zone.
 *
 * @param config Configuration.
 * @param place Place of the element.
 */
export function columnAt(config: PivotRequest, place: ZonePlace): number | undefined {
  return place.zone === 'measures'
    ? config.measures[place.index]?.column
    : config[place.zone][place.index]
}

/**
 * Configuration without one of its elements.
 *
 * @param config Configuration.
 * @param place Place of the element to remove.
 */
export function removeElement(config: PivotRequest, place: ZonePlace): PivotRequest {
  const without = <T>(items: T[]) => items.filter((_, index) => index !== place.index)
  return {
    rows: place.zone === 'rows' ? without(config.rows) : config.rows,
    columns: place.zone === 'columns' ? without(config.columns) : config.columns,
    measures: place.zone === 'measures' ? without(config.measures) : config.measures,
  }
}

/**
 * Places a column in a zone, at a position: a new element, or one moved from another place.
 *
 * A level leaves the axis where it was, if it was a level already. A measure moved stays the
 * same measure; a column that becomes a measure takes its default measure (`defaultMeasure`).
 *
 * @param config Configuration.
 * @param column Index of the column.
 * @param family Family of its type.
 * @param to Zone and position of the element; a position beyond the end places it last.
 * @param from Place of the moved element, `null` for a new element.
 */
export function placeColumn(
  config: PivotRequest,
  column: number,
  family: TypeFamily | undefined,
  to: ZonePlace,
  from: ZonePlace | null,
): PivotRequest {
  const moved = from?.zone === 'measures' ? config.measures[from.index] : undefined
  let result = config
  let index = to.index
  // Removing an element before the target, in the same zone, shifts the target.
  const remove = (place: ZonePlace) => {
    result = removeElement(result, place)
    if (place.zone === to.zone && place.index < index) {
      index--
    }
  }
  if (from !== null) {
    remove(from)
  }
  if (to.zone !== 'measures') {
    for (const zone of ['rows', 'columns'] as const) {
      const existing = result[zone].indexOf(column)
      if (existing >= 0) {
        remove({ zone, index: existing })
      }
    }
  }
  const insert = <T>(items: T[], item: T) => [...items.slice(0, index), item, ...items.slice(index)]
  return {
    rows: to.zone === 'rows' ? insert(result.rows, column) : result.rows,
    columns: to.zone === 'columns' ? insert(result.columns, column) : result.columns,
    measures:
      to.zone === 'measures'
        ? insert(result.measures, moved ?? defaultMeasure(column, family))
        : result.measures,
  }
}

/**
 * Changes the aggregate of a measure.
 *
 * @param config Configuration.
 * @param index Position of the measure.
 * @param aggregate New aggregate.
 */
export function withAggregate(
  config: PivotRequest,
  index: number,
  aggregate: PivotMeasure['aggregate'],
): PivotRequest {
  return {
    ...config,
    measures: config.measures.map((measure, position) =>
      position === index ? { ...measure, aggregate } : measure,
    ),
  }
}

/**
 * Configuration of the Pivoteur for a result tab: the columns of the result placed in the zones
 * Lignes, Colonnes and Mesures (`PivotZones.vue`), the names given to them, and the labels of
 * their elements.
 *
 * In this first slice, a level is a column of the result, by its index, and a measure is
 * `COUNT(*)`, `COUNT(col)`, `COUNT(DISTINCT col)` or `SUM(col)`; the expressions come with the
 * parser. Each element can take a name, its alias (F-17), shown instead of its expression (F-12):
 * the name of its column for a level. A column is a level at most once, on one of the two axes:
 * placing it again moves it, with its name. A measure can be placed several times. Each change
 * gives a new configuration; the panel computes again only when the request to the engine
 * (`engineRequest`) changes, a name changing only the labels.
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
 * Level of the rows or of the columns.
 */
export interface PivotLevel {
  /**
   * Index of the column of the result.
   */
  column: number
  /**
   * Name given to the level, `''` without one: the level then shows the name of its column.
   */
  name: string
}

/**
 * Measure of the configuration.
 */
export interface NamedMeasure extends PivotMeasure {
  /**
   * Name given to the measure, `''` without one: the measure then shows its expression.
   */
  name: string
}

/**
 * Configuration of a tab: the elements of each zone, the outermost level first.
 */
export interface PivotConfig {
  /**
   * Levels of the rows.
   */
  rows: PivotLevel[]
  /**
   * Levels of the columns.
   */
  columns: PivotLevel[]
  /**
   * Measures, in the order of their zone.
   */
  measures: NamedMeasure[]
}

/**
 * Configuration without any element.
 */
export function emptyConfig(): PivotConfig {
  return { rows: [], columns: [], measures: [] }
}

/**
 * Is the configuration without any element?
 *
 * @param config Configuration.
 */
export function isEmptyConfig(config: PivotConfig): boolean {
  return config.rows.length === 0 && config.columns.length === 0 && config.measures.length === 0
}

/**
 * Table to ask the engine for a configuration, without the names, which it does not read.
 *
 * @param config Configuration.
 */
export function engineRequest(config: PivotConfig): PivotRequest {
  return {
    rows: config.rows.map((level) => level.column),
    columns: config.columns.map((level) => level.column),
    measures: config.measures.map(({ aggregate, column }) => ({ aggregate, column })),
  }
}

/**
 * Do two requests ask for the same table?
 *
 * @param a Request.
 * @param b Other request.
 */
export function sameRequest(a: PivotRequest, b: PivotRequest): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
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
 * Measure of a column dropped into the measures: `SUM` for a number, `COUNT` otherwise (F-15),
 * without a name.
 *
 * @param column Index of the column.
 * @param family Family of its type.
 */
export function defaultMeasure(column: number, family: TypeFamily | undefined): NamedMeasure {
  return { aggregate: summable(family) ? 'sum' : 'count', column, name: '' }
}

/**
 * Expression of a measure: `SUM(montant)`, `COUNT(ville)`, `COUNT(DISTINCT ville)`, `COUNT(*)`.
 *
 * @param measure Measure.
 * @param names Names of the columns of the result.
 */
export function measureExpression(measure: PivotMeasure, names: string[]): string {
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
 * Expression of an element: the name of the column of a level, the expression of a measure.
 *
 * @param config Configuration.
 * @param place Place of the element.
 * @param names Names of the columns of the result.
 */
export function elementExpression(config: PivotConfig, place: ZonePlace, names: string[]): string {
  if (place.zone === 'measures') {
    const measure = config.measures[place.index]
    return measure === undefined ? '?' : measureExpression(measure, names)
  }
  return names[config[place.zone][place.index]?.column ?? -1] ?? '?'
}

/**
 * Label of an element: its name, or its expression without one (F-12).
 *
 * @param config Configuration.
 * @param place Place of the element.
 * @param names Names of the columns of the result.
 */
export function elementLabel(config: PivotConfig, place: ZonePlace, names: string[]): string {
  const name = config[place.zone][place.index]?.name ?? ''
  return name === '' ? elementExpression(config, place, names) : name
}

/**
 * Labels of the elements of a zone.
 *
 * @param config Configuration.
 * @param zone Zone.
 * @param names Names of the columns of the result.
 */
export function zoneLabels(config: PivotConfig, zone: PivotZone, names: string[]): string[] {
  return config[zone].map((_, index) => elementLabel(config, { zone, index }, names))
}

/**
 * Column of an element of a zone.
 *
 * @param config Configuration.
 * @param place Place of the element.
 */
export function columnAt(config: PivotConfig, place: ZonePlace): number | undefined {
  return config[place.zone][place.index]?.column
}

/**
 * Configuration without one of its elements.
 *
 * @param config Configuration.
 * @param place Place of the element to remove.
 */
export function removeElement(config: PivotConfig, place: ZonePlace): PivotConfig {
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
 * A level leaves the axis where it was, if it was a level already, and keeps its name. A measure
 * moved stays the same measure, with its name; a column that becomes a measure takes its default
 * measure (`defaultMeasure`), and a measure that becomes a level loses its name.
 *
 * @param config Configuration.
 * @param column Index of the column.
 * @param family Family of its type.
 * @param to Zone and position of the element; a position beyond the end places it last.
 * @param from Place of the moved element, `null` for a new element.
 */
export function placeColumn(
  config: PivotConfig,
  column: number,
  family: TypeFamily | undefined,
  to: ZonePlace,
  from: ZonePlace | null,
): PivotConfig {
  const moved = from?.zone === 'measures' ? config.measures[from.index] : undefined
  let level = from !== null && from.zone !== 'measures' ? config[from.zone][from.index] : undefined
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
      const existing = result[zone].findIndex((item) => item.column === column)
      if (existing >= 0) {
        level ??= result[zone][existing]
        remove({ zone, index: existing })
      }
    }
  }
  const insert = <T>(items: T[], item: T) => [...items.slice(0, index), item, ...items.slice(index)]
  const newLevel = level ?? { column, name: '' }
  return {
    rows: to.zone === 'rows' ? insert(result.rows, newLevel) : result.rows,
    columns: to.zone === 'columns' ? insert(result.columns, newLevel) : result.columns,
    measures:
      to.zone === 'measures'
        ? insert(result.measures, moved ?? defaultMeasure(column, family))
        : result.measures,
  }
}

/**
 * Changes the aggregate of a measure, which keeps its name.
 *
 * @param config Configuration.
 * @param index Position of the measure.
 * @param aggregate New aggregate.
 */
export function withAggregate(
  config: PivotConfig,
  index: number,
  aggregate: PivotMeasure['aggregate'],
): PivotConfig {
  return {
    ...config,
    measures: config.measures.map((measure, position) =>
      position === index ? { ...measure, aggregate } : measure,
    ),
  }
}

/**
 * Names an element. A name empty, or equal to the expression of the element, removes its name:
 * the element shows its expression again.
 *
 * @param config Configuration.
 * @param place Place of the element.
 * @param name Name typed.
 * @param names Names of the columns of the result.
 */
export function withName(
  config: PivotConfig,
  place: ZonePlace,
  name: string,
  names: string[],
): PivotConfig {
  const trimmed = name.trim()
  const kept = trimmed === elementExpression(config, place, names) ? '' : trimmed
  const rename = <T extends { name: string }>(items: T[]) =>
    items.map((item, index) => (index === place.index ? { ...item, name: kept } : item))
  return {
    rows: place.zone === 'rows' ? rename(config.rows) : config.rows,
    columns: place.zone === 'columns' ? rename(config.columns) : config.columns,
    measures: place.zone === 'measures' ? rename(config.measures) : config.measures,
  }
}

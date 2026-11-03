/**
 * Configuration of the Pivoteur for a result tab: the columns of the result placed in the zones
 * Lignes, Colonnes, Mesures and Filtres (`PivotZones.vue`), the names given to them, and the labels
 * of their elements.
 *
 * In this first slice, a level is a column of the result, by its index, and a measure is
 * `COUNT(*)` or an aggregate of a column (`measureExpression`); the expressions come with the
 * parser. Each element can take a name, its alias (F-17), shown instead of its expression (F-12):
 * the name of its column for a level. A level of dates takes a precision, what it keeps of them
 * (`precisionChoices`). A column can be placed several times, in a zone and on both axes, each
 * element on its own: `d (Année)` in the columns and `d (Mois)` in the rows. Each change gives a
 * new configuration; the panel computes again only when the request to the engine (`engineRequest`)
 * changes, a name changing only the labels, a sort only their layout (`engineOrder`).
 *
 * A level has its direction (C-11), which it keeps wherever it goes; on each axis, the values of a
 * node of the other axis can rank the nodes instead (C-13), named by their values. Such a sort
 * lasts as long as the levels of that other axis and its measure (`withFollowedTargets`). A column
 * of values keeps the width given to it, named the same way, as long as the levels of the columns
 * stay, and follows its measure; a level keeps the width of its column of labels wherever it goes
 * (`withColumnWidth`, `withLabelWidth`).
 *
 * The lines take a form (C-07), and repeat the labels of their groups if asked. Each axis shows its
 * grand total and the subtotal of each level if asked, a setting that follows the level wherever
 * it goes (F-36). All of it only lays the table out again (`engineLayout`).
 *
 * A filter (F-19) keeps the rows whose value of a column, at a precision of its dates, passes a
 * condition: a list of values (F-21), or a test (`PivotTest`); or those of the groups whose measure
 * passes a test (F-44). Its groups are those of a level of an axis with the levels above it, by
 * default the deepest level of the rows (`filterLevels`). A filter that lacks a value has no effect
 * (`activeFilters`). A column dragged from an axis moves into the filters with its precision, a
 * measure dragged there is copied, and a column of the list gives a new filter each time; numbers
 * and dates start on `BETWEEN`, the other columns on the list (`placeFilter`).
 *
 * The configuration is saved with its tab, whose query can change: it keeps the names of the
 * columns it was made for, and follows them in the columns of a new result (`fittedConfig`). Its
 * types are those of the server (`crates/server/src/tab_pivot.rs`).
 */

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { DatePrecision } from '@/api/generated/DatePrecision'
import type { PivotColumnWidth } from '@/api/generated/PivotColumnWidth'
import type { PivotCondition } from '@/api/generated/PivotCondition'
import type { PivotConfig } from '@/api/generated/PivotConfig'
import type { PivotForm } from '@/api/generated/PivotForm'
import type { PivotFilter } from '@/api/generated/PivotFilter'
import type { PivotLevel } from '@/api/generated/PivotLevel'
import type { PivotTest } from '@/api/generated/PivotTest'
import type { PivotValueSort } from '@/api/generated/PivotValueSort'
import type { TabPivot } from '@/api/generated/TabPivot'
import type { TypeFamily } from '@/api/generated/TypeFamily'

import type {
  PivotAxisOrder,
  PivotAxisTotals,
  PivotLayout,
  PivotMeasure,
  PivotOrder,
  PivotRequest,
  PivotRequestFilter,
  PivotRequestLevel,
  PivotValueTarget,
} from './pivotEngine'
import { sameColumns } from './resultGrid'

export type { DatePrecision } from '@/api/generated/DatePrecision'
export type { NamedMeasure } from '@/api/generated/NamedMeasure'
export type { PivotCondition, PivotConfig, PivotFilter, PivotLevel, PivotTest }

/**
 * Filter on a measure.
 */
export type PivotMeasureFilter = Extract<PivotFilter, { kind: 'measure' }>

/**
 * Filter on a column.
 */
export type PivotColumnFilter = Extract<PivotFilter, { kind: 'column' }>

/**
 * Zone of the panel.
 */
export type PivotZone = 'rows' | 'columns' | 'measures' | 'filters'

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
 * Axis of the table, by the zone of its levels.
 */
export type PivotAxis = 'rows' | 'columns'

/**
 * Field of the configuration that holds the sort by values of each axis.
 */
const VALUE_SORT = { rows: 'sort_lines', columns: 'sort_columns' } as const

/**
 * Field of the configuration that leaves out the grand total of each axis.
 */
const WITHOUT_TOTAL = { rows: 'without_row_total', columns: 'without_column_total' } as const

/**
 * Label of a precision in the menu of a level.
 */
export type PrecisionLabel = 'Année' | 'Trimestre' | 'Mois' | 'Semaine' | 'Date' | 'Date et heure'

/**
 * Precision a level can take.
 */
export interface PrecisionChoice {
  /**
   * What the level keeps of the dates of its column.
   */
  precision: DatePrecision
  /**
   * Label of the choice.
   */
  label: PrecisionLabel
}

/**
 * Precisions a level of this family can take, from the coarsest to the finest, the values of the
 * column as they are last: a date is its own "Date". The year and the week are those of ISO 8601,
 * so that 2027-01-01 falls in 2026 and in its week 53.
 *
 * @param family
 *     Family of the type of the column; unknown for a result saved before its columns were
 *     typed.
 * @returns The choices, none for a column that holds no dates (`year` and `time` included).
 */
export function precisionChoices(family: TypeFamily | undefined): PrecisionChoice[] {
  const periods: PrecisionChoice[] = [
    { precision: 'year', label: 'Année' },
    { precision: 'quarter', label: 'Trimestre' },
    { precision: 'month', label: 'Mois' },
    { precision: 'week', label: 'Semaine' },
  ]
  switch (family) {
    case 'date':
      return [...periods, { precision: 'none', label: 'Date' }]
    case 'datetime':
      return [
        ...periods,
        { precision: 'date', label: 'Date' },
        { precision: 'none', label: 'Date et heure' },
      ]
    default:
      return []
  }
}

/**
 * Configuration without any element.
 */
export function emptyConfig(): PivotConfig {
  return { rows: [], columns: [], measures: [], filters: [] }
}

/**
 * Is the configuration without any element?
 *
 * @param config Configuration.
 */
export function isEmptyConfig(config: PivotConfig): boolean {
  return !hasTable(config) && config.filters.length === 0
}

/**
 * Does the configuration ask for a table? Filters alone do not.
 *
 * @param config Configuration.
 */
export function hasTable(config: PivotConfig): boolean {
  return config.rows.length > 0 || config.columns.length > 0 || config.measures.length > 0
}

/**
 * Where each column of a result went in the columns of a new one, by name, the n-th of a repeated
 * name going to the n-th of that name.
 *
 * @param fields Names of the columns of the former result.
 * @param columns Names of the columns of the new result.
 * @returns Index of each former column in the new result, absent for a lost column.
 */
function movedColumns(fields: string[], columns: string[]): (number | undefined)[] {
  const free = new Map<string, number[]>()
  columns.forEach((name, index) => free.set(name, [...(free.get(name) ?? []), index]))
  return fields.map((name) => free.get(name)?.shift())
}

/**
 * Configuration whose levels follow their columns by name; the element of a lost column leaves,
 * except `COUNT(*)`, which reads no column, and takes the first one for its menu. A filter whose
 * level is lost goes back to the default level.
 *
 * @param pivot Configuration, with the columns it was made for.
 * @param columns Names of the columns of the result.
 */
function followedConfig(pivot: TabPivot, columns: string[]): PivotConfig {
  const { fields, config } = pivot
  const { sort_lines, sort_columns, column_widths, ...settings } = config
  const moved = movedColumns(fields, columns)
  const follow = <T extends { column: number }>(item: T, column: number | undefined) =>
    column === undefined ? [] : [{ ...item, column }]
  const levels = (items: PivotConfig['rows']) =>
    items.flatMap((item) => follow(item, moved[item.column]))
  const followed: PivotConfig = {
    ...settings,
    rows: levels(config.rows),
    columns: levels(config.columns),
    measures: config.measures.flatMap((measure) =>
      follow(
        measure,
        moved[measure.column] ?? (measure.aggregate === 'count_star' ? 0 : undefined),
      ),
    ),
    filters: config.filters.flatMap((filter): PivotFilter[] => {
      if (filter.kind === 'column') {
        return follow(filter, moved[filter.column])
      }
      const column = moved[filter.column] ?? (filter.aggregate === 'count_star' ? 0 : undefined)
      const level = filter.level === undefined ? undefined : moved[filter.level]
      const level_precision = level === undefined ? 'none' : filter.level_precision
      return follow({ ...filter, level, level_precision }, column)
    }),
  }
  const whole =
    followed.rows.length === config.rows.length &&
    followed.columns.length === config.columns.length &&
    followed.measures.length === config.measures.length
  return whole ? { ...followed, sort_lines, sort_columns, column_widths } : followed
}

/**
 * Configuration whose levels and filters keep only the precisions their columns can take: a column
 * that holds no dates any more gives its values as they are. A filter loses what it compared to,
 * the values of a period having no meaning for the values as they are. A column without a type
 * keeps its precision.
 *
 * @param config Configuration.
 * @param types Type of each column of the result.
 * @returns The configuration itself when each level keeps its precision.
 */
function withPossiblePrecisions(config: PivotConfig, types: ColumnTypeInfo[]): PivotConfig {
  const possible = ({ column, precision }: Pick<PivotLevel, 'column' | 'precision'>) => {
    const type = types[column]
    return (
      precision === 'none' ||
      type === undefined ||
      precisionChoices(type.family).some((choice) => choice.precision === precision)
    )
  }
  const fit = (levels: PivotLevel[]) =>
    levels.map((level): PivotLevel => (possible(level) ? level : { ...level, precision: 'none' }))
  const filters = config.filters.map((filter): PivotFilter => {
    if (filter.kind === 'column') {
      return possible(filter)
        ? filter
        : { ...filter, precision: 'none', values: [], operand: '', upper: '' }
    }
    // A filter goes on reading the level whose precision leaves.
    const fits =
      filter.level === undefined ||
      possible({ column: filter.level, precision: filter.level_precision })
    return fits ? filter : { ...filter, level_precision: 'none' }
  })
  const fitted = filters.every((filter, index) => filter === config.filters[index])
  if (fitted && [...config.rows, ...config.columns].every(possible)) {
    return config
  }
  return { ...config, rows: fit(config.rows), columns: fit(config.columns), filters }
}

/**
 * Configuration of a tab fitted to the columns of its result, which may have changed since it was
 * made: their names (`followedConfig`) and their types (`withPossiblePrecisions`).
 *
 * @param pivot Configuration, with the columns it was made for.
 * @param columns Names of the columns of the result.
 * @param types
 *     Type of each column of the result; none for a result saved before its columns were typed.
 * @returns The configuration itself when there is nothing to fit.
 */
export function fittedConfig(
  pivot: TabPivot,
  columns: string[],
  types: ColumnTypeInfo[],
): PivotConfig {
  if (isEmptyConfig(pivot.config)) {
    return pivot.config
  }
  const followed = sameColumns(pivot.fields, columns)
    ? pivot.config
    : followedConfig(pivot, columns)
  return withPossiblePrecisions(followed, types)
}

/**
 * Level a filter on a measure was given, by its column and its precision, when it is still on an
 * axis. The same level placed twice reads the first.
 *
 * @param config Configuration.
 * @param filter Filter.
 * @returns The level, none for the default level.
 */
export function chosenLevel(
  config: PivotConfig,
  filter: PivotMeasureFilter,
): PivotLevel | undefined {
  return [...config.rows, ...config.columns].find(
    ({ column, precision }) => column === filter.level && precision === filter.level_precision,
  )
}

/**
 * Levels whose values make the groups of a filter on a measure (F-44): its level and those above
 * it; by default all those of the rows, then, without any, all those of the columns. Without any
 * level, all the rows make one group.
 *
 * @param config Configuration.
 * @param filter Filter.
 */
export function filterLevels(config: PivotConfig, filter: PivotMeasureFilter): PivotLevel[] {
  const level = chosenLevel(config, filter)
  for (const axis of [config.rows, config.columns]) {
    const index = axis.findIndex(
      ({ column, precision }) => column === level?.column && precision === level?.precision,
    )
    if (index >= 0) {
      return axis.slice(0, index + 1)
    }
  }
  return config.rows.length > 0 ? config.rows : config.columns
}

/**
 * Number of values a test compares to.
 *
 * @param condition Condition of a filter.
 * @returns 2 for bounds, 1 for an operand, 0 for `IS NULL`, `IS NOT NULL` and the lists.
 */
export function operandCount(condition: PivotCondition): 0 | 1 | 2 {
  switch (condition) {
    case 'between':
    case 'not_between':
      return 2
    case 'in_list':
    case 'not_in_list':
    case 'is_null':
    case 'is_not_null':
      return 0
    default:
      return 1
  }
}

/**
 * Does a filter have an effect? A list needs a value, and a test its operands. `NOT IN` writes no
 * NULL: as in SQL, it would keep no row, which a click must not do by accident.
 *
 * @param filter Filter.
 */
function hasEffect(filter: PivotFilter): boolean {
  switch (filter.condition) {
    case 'in_list':
      return filter.values.length > 0
    case 'not_in_list':
      return filter.values.some((value) => value !== null)
    case 'is_null':
    case 'is_not_null':
      return true
    default:
      return filter.operand !== '' && (operandCount(filter.condition) === 1 || filter.upper !== '')
  }
}

/**
 * Filters that have an effect (`hasEffect`). The engine reads only these and names them by their
 * rank here.
 *
 * @param config Configuration.
 */
export function activeFilters(config: PivotConfig): PivotFilter[] {
  return config.filters.filter(hasEffect)
}

/**
 * Place of a filter the engine names by its rank.
 *
 * @param config Configuration.
 * @param rank Rank of the filter among those that have an effect (`activeFilters`).
 */
export function activeFilterPlace(config: PivotConfig, rank: number): ZonePlace {
  const filter = activeFilters(config)[rank]
  return { zone: 'filters', index: filter === undefined ? -1 : config.filters.indexOf(filter) }
}

/**
 * Table to ask the engine for a configuration, without the names, which it does not read. It copies
 * what it reads of the configuration, which is reactive and could not go to the Worker.
 *
 * @param config Configuration.
 */
export function engineRequest(config: PivotConfig): PivotRequest {
  const levels = (items: PivotLevel[]) =>
    items.map(({ column, precision }): PivotRequestLevel => ({ column, precision }))
  const filter = (item: PivotFilter): PivotRequestFilter => {
    const { condition, operand, upper } = item
    if (item.kind === 'measure') {
      return {
        kind: 'measure',
        measure: { aggregate: item.aggregate, column: item.column },
        levels: levels(filterLevels(config, item)),
        test: { condition: item.condition, operand, upper },
      }
    }
    const level = { column: item.column, precision: item.precision }
    switch (condition) {
      case 'in_list':
        return { kind: 'values', level, values: [...item.values], excluded: false }
      case 'not_in_list': {
        const values = item.values.filter((value) => value !== null)
        return { kind: 'values', level, values, excluded: true }
      }
      default:
        return { kind: 'column', level, test: { condition, operand, upper } }
    }
  }
  return {
    rows: levels(config.rows),
    columns: levels(config.columns),
    measures: config.measures.map(({ aggregate, column }) => ({ aggregate, column })),
    filters: activeFilters(config).map(filter),
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
 * Order to ask the engine for a configuration. It copies what it reads of the configuration, which
 * is reactive and could not go to the Worker.
 *
 * @param config Configuration.
 */
export function engineOrder(config: PivotConfig): PivotOrder {
  const axis = (levels: PivotLevel[], sort: PivotValueSort | undefined): PivotAxisOrder => ({
    // All ascending: the order of a table just computed.
    descending: levels.some(({ descending }) => descending)
      ? levels.map(({ descending }) => descending === true)
      : [],
    values:
      sort === undefined
        ? null
        : { path: [...sort.path], measure: sort.measure, descending: sort.descending },
  })
  return {
    rows: axis(config.rows, config.sort_lines),
    columns: axis(config.columns, config.sort_columns),
  }
}

/**
 * Columns of values given a width, named by their values, whose positions the engine gives with the
 * table: copies of those of the configuration, which the Worker can receive.
 *
 * @param config Configuration.
 */
export function engineTargets(config: PivotConfig): PivotValueTarget[] {
  return (config.column_widths ?? []).map(({ path, measure }) => ({ path: [...path], measure }))
}

/**
 * Do two orders ask for the same layout?
 *
 * @param a Order.
 * @param b Other order.
 */
export function sameOrder(a: PivotOrder, b: PivotOrder): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/**
 * Totals of an axis, as the engine reads them.
 *
 * @param config Configuration.
 * @param axis Axis.
 */
function axisTotals(config: PivotConfig, axis: PivotAxis): PivotAxisTotals {
  const levels = config[axis]
  return {
    grandTotal: hasGrandTotal(config, axis),
    // Every subtotal: the layout of a table just computed.
    subtotals: levels.some(({ without_subtotal }) => without_subtotal)
      ? levels.map(({ without_subtotal }) => without_subtotal !== true)
      : [],
  }
}

/**
 * Layout to ask the engine for a configuration. It copies what it reads of the configuration,
 * which is reactive and could not go to the Worker.
 *
 * @param config Configuration.
 */
export function engineLayout(config: PivotConfig): PivotLayout {
  return {
    form: config.form ?? 'compact',
    rows: axisTotals(config, 'rows'),
    columns: axisTotals(config, 'columns'),
  }
}

/**
 * Do two layouts lay a table out in the same way?
 *
 * @param a Layout.
 * @param b Other layout.
 */
export function sameTableLayout(a: PivotLayout, b: PivotLayout): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/**
 * Does an axis show its grand total?
 *
 * @param config Configuration.
 * @param axis Axis.
 */
export function hasGrandTotal(config: PivotConfig, axis: PivotAxis): boolean {
  return config[WITHOUT_TOTAL[axis]] !== true
}

/**
 * Configuration with lines in a form.
 *
 * @param config Configuration.
 * @param form Form of the lines.
 */
export function withForm(config: PivotConfig, form: PivotForm): PivotConfig {
  return { ...config, form }
}

/**
 * Configuration whose lines repeat the labels of their groups, or not.
 *
 * @param config Configuration.
 * @param repeat Do the lines repeat them?
 */
export function withRepeatedLabels(config: PivotConfig, repeat: boolean): PivotConfig {
  return { ...config, repeat_labels: repeat }
}

/**
 * Configuration where an axis shows its grand total, or not.
 *
 * @param config Configuration.
 * @param axis Axis.
 * @param shown Is it shown?
 */
export function withGrandTotal(config: PivotConfig, axis: PivotAxis, shown: boolean): PivotConfig {
  return { ...config, [WITHOUT_TOTAL[axis]]: !shown }
}

/**
 * Configuration where a level shows its subtotal, or not.
 *
 * @param config Configuration.
 * @param axis Axis of the level.
 * @param index Position of the level.
 * @param shown Is it shown?
 */
export function withSubtotal(
  config: PivotConfig,
  axis: PivotAxis,
  index: number,
  shown: boolean,
): PivotConfig {
  return {
    ...config,
    [axis]: config[axis].map((level, position) =>
      position === index ? { ...level, without_subtotal: !shown } : level,
    ),
  }
}

/**
 * Configuration where every level of an axis shows its subtotal, or none does.
 *
 * @param config Configuration.
 * @param axis Axis of the levels.
 * @param shown Are they shown?
 */
export function withAllSubtotals(
  config: PivotConfig,
  axis: PivotAxis,
  shown: boolean,
): PivotConfig {
  return {
    ...config,
    [axis]: config[axis].map((level) => ({ ...level, without_subtotal: !shown })),
  }
}

/**
 * Do these values name the same node and the same measure?
 *
 * @param a Target.
 * @param b Other target.
 */
export function sameTarget(a: PivotValueTarget, b: PivotValueTarget): boolean {
  return (
    a.measure === b.measure &&
    a.path.length === b.path.length &&
    a.path.every((value, index) => value === b.path[index])
  )
}

/**
 * Configuration after a click on the sort of a level: the level inverts its direction. When values
 * rank its axis, the click gives the axis back to its labels, in the direction they had.
 *
 * @param config Configuration.
 * @param axis Axis of the level.
 * @param index Position of the level.
 */
export function withLevelSort(config: PivotConfig, axis: PivotAxis, index: number): PivotConfig {
  if (config[VALUE_SORT[axis]] !== undefined) {
    return { ...config, [VALUE_SORT[axis]]: undefined }
  }
  return {
    ...config,
    [axis]: config[axis].map((level, position) =>
      position === index ? { ...level, descending: !level.descending } : level,
    ),
  }
}

/**
 * Configuration after a click on the sort of a node of the other axis, as in the grid: ascending,
 * then descending, then none.
 *
 * @param config Configuration.
 * @param axis Axis the values rank.
 * @param target Node and measure of the values.
 */
export function withValueSort(
  config: PivotConfig,
  axis: PivotAxis,
  target: PivotValueTarget,
): PivotConfig {
  const current = config[VALUE_SORT[axis]]
  const again = current !== undefined && sameTarget(current, target)
  const next = again && current.descending ? undefined : { ...target, descending: again }
  return { ...config, [VALUE_SORT[axis]]: next }
}

/**
 * Do two lists of levels group the rows in the same way? Their names and directions apart.
 *
 * @param a Levels.
 * @param b Other levels.
 */
function sameLevels(a: PivotLevel[], b: PivotLevel[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (level, index) =>
        level.column === b[index]?.column && level.precision === b[index]?.precision,
    )
  )
}

/**
 * Keeps the sorts by values and the widths of the columns of values that still read the same
 * values after a change of the zones: a column ranks the lines and keeps its width as long as the
 * levels of the columns stay, and follows its measure; a line ranks the columns as long as the
 * levels of the rows stay, by the first measure.
 *
 * @param before Configuration before the change.
 * @param after Configuration after the change.
 */
function withFollowedTargets(before: PivotConfig, after: PivotConfig): PivotConfig {
  const followed = (index: number) => {
    const measure = before.measures[index]
    return after.measures.findIndex(
      ({ aggregate, column }) => aggregate === measure?.aggregate && column === measure?.column,
    )
  }
  const columns = sameLevels(before.columns, after.columns)
  const lines = before.sort_lines
  const measure = lines === undefined ? -1 : followed(lines.measure)
  const widths = (columns ? (before.column_widths ?? []) : []).flatMap(
    (width): PivotColumnWidth[] => {
      const moved = followed(width.measure)
      return moved >= 0 ? [{ ...width, measure: moved }] : []
    },
  )
  return {
    ...after,
    sort_lines: lines && measure >= 0 && columns ? { ...lines, measure } : undefined,
    sort_columns:
      after.measures.length > 0 && sameLevels(before.rows, after.rows)
        ? before.sort_columns
        : undefined,
    column_widths: widths.length > 0 ? widths : undefined,
  }
}

/**
 * Configuration after a column of values takes a width, or gives it back. Named by its values, the
 * column keeps it while the table does not show it.
 *
 * @param config Configuration.
 * @param target Values of the node of the column, and its measure (0 without measures).
 * @param width Width, in steps of the spacing scale; `null` for the default width.
 */
export function withColumnWidth(
  config: PivotConfig,
  target: PivotValueTarget,
  width: number | null,
): PivotConfig {
  const others = (config.column_widths ?? []).filter((column) => !sameTarget(column, target))
  const widths =
    width === null ? others : [...others, { path: target.path, measure: target.measure, width }]
  return { ...config, column_widths: widths.length > 0 ? widths : undefined }
}

/**
 * Configuration after the labels of the lines take a width, or give it back: that of their single
 * column, or that of the column of a level, which the level keeps wherever it goes.
 *
 * @param config Configuration.
 * @param level Level of the rows whose column it is; `null` for the labels in a single column.
 * @param width Width, in steps of the spacing scale; `null` for the default width.
 */
export function withLabelWidth(
  config: PivotConfig,
  level: number | null,
  width: number | null,
): PivotConfig {
  if (level === null) {
    return { ...config, labels_width: width ?? undefined }
  }
  return {
    ...config,
    rows: config.rows.map((row, index) =>
      index === level ? { ...row, width: width ?? undefined } : row,
    ),
  }
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
 * Does an aggregate add up the numbers of its column? `SUM` and `AVG`, with or without `DISTINCT`.
 *
 * @param aggregate Aggregate.
 */
export function addsNumbers(aggregate: PivotMeasure['aggregate']): boolean {
  return ['sum', 'sum_distinct', 'avg', 'avg_distinct'].includes(aggregate)
}

/**
 * Aggregate of a column dropped into the measures. The sum of a key means nothing: `COUNT(*)` for
 * a column of a primary key, `COUNT(col)` for a foreign key, which can be NULL. Otherwise `SUM`
 * for a number, `COUNT` for the others (F-15).
 *
 * @param type Type of the column; none for a result saved before its columns were typed.
 * @param foreignKey Is the column part of a foreign key of its table?
 */
export function defaultAggregate(
  type: ColumnTypeInfo | undefined,
  foreignKey: boolean,
): PivotMeasure['aggregate'] {
  if (type?.origin?.primary_key) {
    return 'count_star'
  }
  return summable(type?.family) && !foreignKey ? 'sum' : 'count'
}

/**
 * Expression of a measure: `SUM(montant)`, `COUNT(DISTINCT ville)`, `MIN(ville)`, `COUNT(*)`.
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
    case 'sum_distinct':
      return `SUM(DISTINCT ${name})`
    case 'avg':
      return `AVG(${name})`
    case 'avg_distinct':
      return `AVG(DISTINCT ${name})`
    case 'min':
      return `MIN(${name})`
    case 'max':
      return `MAX(${name})`
  }
}

/**
 * Sign of each comparison, as ANSI SQL writes it.
 */
const COMPARISON_SIGNS = {
  equal: '=',
  not_equal: '<>',
  less: '<',
  less_or_equal: '<=',
  greater: '>',
  greater_or_equal: '>=',
}

/**
 * Family of the values of a filter at a precision, for their literals: a year is a number, a day a
 * date, and the other periods are texts.
 */
const PERIOD_FAMILIES: Record<Exclude<DatePrecision, 'none'>, TypeFamily> = {
  year: 'integer',
  quarter: 'text',
  month: 'text',
  week: 'text',
  date: 'date',
}

/**
 * Values an expression of a filter writes at most.
 */
const LISTED_VALUES = 20

/**
 * Literal of a value in an expression: a number as it is, a date or an instant typed (D23), a text
 * between quotes, its quotes doubled.
 *
 * @param value Value.
 * @param family Family of the type of the value.
 */
function sqlLiteral(value: string, family: TypeFamily | undefined): string {
  switch (family) {
    case 'integer':
    case 'decimal':
    case 'float':
    case 'year':
      return value
    case 'date':
      return `DATE'${value}'`
    case 'datetime':
      return `TIMESTAMP'${value}'`
    case 'time':
      return `TIME'${value}'`
    default:
      return `'${value.replace(/'/g, "''")}'`
  }
}

/**
 * Family of the values of a measure: an integer for a count, that of its column otherwise; a sum
 * or an average holds numbers, even of a column without a type.
 *
 * @param measure Measure.
 * @param types Type of each column of the result.
 */
function measureFamily(
  { aggregate, column }: PivotMeasure,
  types: ColumnTypeInfo[],
): TypeFamily | undefined {
  return ['count_star', 'count', 'count_distinct'].includes(aggregate)
    ? 'integer'
    : (types[column]?.family ?? (addsNumbers(aggregate) ? 'decimal' : undefined))
}

/**
 * Name of a column with its precision: `vendue_le (Mois)`.
 *
 * @param name Name of the column.
 * @param precision What it keeps of its dates.
 */
function withPrecisionLabel(name: string, precision: DatePrecision): string {
  if (precision === 'none') {
    return name
  }
  const choice = precisionChoices('datetime').find((item) => item.precision === precision)
  return `${name} (${choice?.label})`
}

/**
 * Expression of a test: `montant BETWEEN 1 AND 5`, `x IS NULL`, `?` for an operand still to give.
 *
 * @param subject Expression of the value.
 * @param test What the value passes, and what it compares to.
 * @param family Family of the type of the value.
 */
function testExpression(
  subject: string,
  { condition, operand, upper }: { condition: PivotTest; operand: string; upper: string },
  family: TypeFamily | undefined,
): string {
  const literal = (value: string) => (value === '' ? '?' : sqlLiteral(value, family))
  switch (condition) {
    case 'between':
      return `${subject} BETWEEN ${literal(operand)} AND ${literal(upper)}`
    case 'not_between':
      return `${subject} NOT BETWEEN ${literal(operand)} AND ${literal(upper)}`
    case 'is_null':
      return `${subject} IS NULL`
    case 'is_not_null':
      return `${subject} IS NOT NULL`
    default:
      return `${subject} ${COMPARISON_SIGNS[condition]} ${literal(operand)}`
  }
}

/**
 * Expression of a filter: `region IN ('Nord', 'Sud')`, `region IN ('Nord') OR region IS NULL`,
 * `region NOT IN ('Nord')`, `d (Mois) BETWEEN '2026-01' AND '2026-03'`, `SUM(montant) > 100`.
 * Without a value, the name of its column, or `?` as its operand.
 *
 * @param filter Filter.
 * @param names Names of the columns of the result.
 * @param types Type of each column of the result.
 */
function filterExpression(filter: PivotFilter, names: string[], types: ColumnTypeInfo[]): string {
  if (filter.kind === 'measure') {
    const { condition, operand, upper } = filter
    return testExpression(
      measureExpression(filter, names),
      { condition, operand, upper },
      measureFamily(filter, types),
    )
  }
  const name = withPrecisionLabel(names[filter.column] ?? '?', filter.precision)
  const family =
    filter.precision === 'none' ? types[filter.column]?.family : PERIOD_FAMILIES[filter.precision]
  const { condition, operand, upper } = filter
  if (condition !== 'in_list' && condition !== 'not_in_list') {
    return testExpression(name, { condition, operand, upper }, family)
  }
  const texts = filter.values.filter((value) => value !== null)
  const hasNull = condition === 'in_list' && texts.length < filter.values.length
  if (texts.length === 0) {
    return hasNull ? `${name} IS NULL` : name
  }
  const literals = texts.slice(0, LISTED_VALUES).map((value) => sqlLiteral(value, family))
  const list = texts.length > LISTED_VALUES ? [...literals, '…'] : literals
  const inList = `${name} ${condition === 'in_list' ? 'IN' : 'NOT IN'} (${list.join(', ')})`
  return hasNull ? `${inList} OR ${name} IS NULL` : inList
}

/**
 * Exact number, as the engine reads it.
 */
const EXACT_NUMBER = /^[+-]?(\d+(\.\d*)?|\.\d+)$/

/**
 * Float, as the engine reads it.
 */
const FLOAT_NUMBER = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/

/**
 * Date of the calendar, years 0001 to 9999.
 */
function isCalendarDate(text: string): boolean {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(text)
  if (parts === null) {
    return false
  }
  const [year, month, day] = parts.slice(1).map(Number) as [number, number, number]
  // The years 2000 to 2399 have the leap years of any cycle of 400 years.
  const days = new Date(Date.UTC(2000 + (year % 400), month, 0)).getUTCDate()
  return year >= 1 && month >= 1 && month <= 12 && day >= 1 && day <= days
}

/**
 * Hour of the day, as the engine reads it: `25:00:00` stops a computation, and cannot be typed.
 */
const HOUR = /^([01]\d|2[0-3]):[0-5]\d:[0-5]\d(\.\d+)?$/

/**
 * Form of the values a filter compares to.
 */
interface ValueForm {
  /**
   * Does a text have this form?
   */
  accepts: (text: string) => boolean
  /**
   * Message of the refusal of another text.
   */
  refusal: string
  /**
   * Text of this form, shown in an empty field.
   */
  example: string
}

/**
 * Form of the values of a family; none for a text, which compares whatever it holds.
 *
 * @param family Family of the type of the values.
 */
function familyForm(family: TypeFamily | undefined): ValueForm | null {
  switch (family) {
    case 'integer':
    case 'decimal':
      return {
        accepts: (text) => EXACT_NUMBER.test(text),
        refusal: 'Un nombre est attendu, comme 100 ou 12.5',
        example: '100',
      }
    case 'float':
      return {
        accepts: (text) => FLOAT_NUMBER.test(text),
        refusal: 'Un nombre est attendu, comme 100 ou 1.5e3',
        example: '100',
      }
    case 'date':
      return {
        accepts: isCalendarDate,
        refusal: 'Une date est attendue, comme 2026-01-15',
        example: '2026-01-15',
      }
    case 'datetime':
      return {
        accepts: (text) =>
          isCalendarDate(text.slice(0, 10)) && text[10] === ' ' && HOUR.test(text.slice(11)),
        refusal: 'Un instant est attendu, comme 2026-01-15 10:30:00',
        example: '2026-01-15 10:30:00',
      }
    case 'time':
      return {
        accepts: (text) => HOUR.test(text),
        refusal: 'Une heure est attendue, comme 10:30:00',
        example: '10:30:00',
      }
    case 'year':
      return {
        accepts: (text) => /^\d{4}$/.test(text),
        refusal: 'Une année est attendue, comme 2026',
        example: '2026',
      }
    default:
      return null
  }
}

/**
 * Form of the labels of a precision, which sort in the order of time.
 *
 * @param precision Precision of a filter.
 */
function periodForm(precision: Exclude<DatePrecision, 'none'>): ValueForm {
  switch (precision) {
    case 'year':
      return {
        accepts: (text) => /^(?!0000)\d{4}$/.test(text),
        refusal: 'Une année est attendue, comme 2026',
        example: '2026',
      }
    case 'quarter':
      return {
        accepts: (text) => /^(?!0000)\d{4}-T[1-4]$/.test(text),
        refusal: 'Un trimestre est attendu, comme 2026-T1',
        example: '2026-T1',
      }
    case 'month':
      return {
        accepts: (text) => /^(?!0000)\d{4}-(0[1-9]|1[0-2])$/.test(text),
        refusal: 'Un mois est attendu, comme 2026-01',
        example: '2026-01',
      }
    case 'week':
      return {
        accepts: (text) => /^(?!0000)\d{4}-S(0[1-9]|[1-4]\d|5[0-3])$/.test(text),
        refusal: 'Une semaine est attendue, comme 2026-S03',
        example: '2026-S03',
      }
    case 'date':
      return {
        accepts: isCalendarDate,
        refusal: 'Une date est attendue, comme 2026-01-15',
        example: '2026-01-15',
      }
  }
}

/**
 * Form of the values a filter on a column or on a measure compares to.
 *
 * @param filter Filter, or its measure.
 * @param types Type of each column of the result.
 */
function valueForm(
  filter: PivotMeasure | PivotColumnFilter,
  types: ColumnTypeInfo[],
): ValueForm | null {
  if ('aggregate' in filter) {
    return familyForm(measureFamily(filter, types))
  }
  return filter.precision === 'none'
    ? familyForm(types[filter.column]?.family)
    : periodForm(filter.precision)
}

/**
 * Why a value cannot compare with what a filter reads. A date, an instant, an hour or a period
 * compares as text: its form is required so that the order of the text is that of time.
 *
 * @param filter Filter, or its measure.
 * @param operand Value, not empty.
 * @param types Type of each column of the result.
 * @returns The message of the refusal, `null` for a value accepted.
 */
export function operandRefusal(
  filter: PivotMeasure | PivotColumnFilter,
  operand: string,
  types: ColumnTypeInfo[],
): string | null {
  const form = valueForm(filter, types)
  return form === null || form.accepts(operand) ? null : form.refusal
}

/**
 * Text of the form a filter awaits, shown in an empty field.
 *
 * @param filter Filter.
 * @param types Type of each column of the result.
 * @returns The example, empty for a text.
 */
export function operandExample(filter: PivotFilter, types: ColumnTypeInfo[]): string {
  return valueForm(filter, types)?.example ?? ''
}

/**
 * Does a filter hold a value that cannot compare with what it reads, after a change of its aggregate
 * or of the type of its column?
 *
 * @param filter Filter.
 * @param types Type of each column of the result.
 */
export function isRefused(filter: PivotFilter, types: ColumnTypeInfo[]): boolean {
  const count = operandCount(filter.condition)
  const values = [filter.operand, filter.upper].slice(0, count)
  return values.some((value) => value !== '' && operandRefusal(filter, value, types) !== null)
}

/**
 * Do numbers or dates sit in an order that a range of values can read?
 *
 * @param family Family of the type of the values.
 */
function isRanged(family: TypeFamily | undefined): boolean {
  return (
    summable(family) ||
    family === 'date' ||
    family === 'datetime' ||
    family === 'time' ||
    family === 'year'
  )
}

/**
 * Expression of an element: the name of the column of a level, with its precision
 * (`vendue_le (Mois)`), the expression of a measure or of a filter.
 *
 * @param config Configuration.
 * @param place Place of the element.
 * @param names Names of the columns of the result.
 * @param types Type of each column of the result, for the literals of a filter.
 */
export function elementExpression(
  config: PivotConfig,
  place: ZonePlace,
  names: string[],
  types: ColumnTypeInfo[] = [],
): string {
  if (place.zone === 'measures') {
    const measure = config.measures[place.index]
    return measure === undefined ? '?' : measureExpression(measure, names)
  }
  if (place.zone === 'filters') {
    const filter = config.filters[place.index]
    return filter === undefined ? '?' : filterExpression(filter, names, types)
  }
  const level = config[place.zone][place.index]
  return withPrecisionLabel(names[level?.column ?? -1] ?? '?', level?.precision ?? 'none')
}

/**
 * Label of an element: its name, or its expression without one (F-12).
 *
 * @param config Configuration.
 * @param place Place of the element.
 * @param names Names of the columns of the result.
 * @param types Type of each column of the result, for the literals of a filter.
 */
export function elementLabel(
  config: PivotConfig,
  place: ZonePlace,
  names: string[],
  types: ColumnTypeInfo[] = [],
): string {
  const name = config[place.zone][place.index]?.name ?? ''
  return name === '' ? elementExpression(config, place, names, types) : name
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
 * Configuration without one of its elements, its sorts left as they are.
 *
 * @param config Configuration.
 * @param place Place of the element to remove.
 */
function withoutElement(config: PivotConfig, place: ZonePlace): PivotConfig {
  const without = <T>(items: T[]) => items.filter((_, index) => index !== place.index)
  return {
    ...config,
    rows: place.zone === 'rows' ? without(config.rows) : config.rows,
    columns: place.zone === 'columns' ? without(config.columns) : config.columns,
    measures: place.zone === 'measures' ? without(config.measures) : config.measures,
    filters: place.zone === 'filters' ? without(config.filters) : config.filters,
  }
}

/**
 * Configuration without one of its elements.
 *
 * @param config Configuration.
 * @param place Place of the element to remove.
 */
export function removeElement(config: PivotConfig, place: ZonePlace): PivotConfig {
  return withFollowedTargets(config, withoutElement(config, place))
}

/**
 * Places a column in the filters, at a position: a filter moved among the filters, a measure copied
 * into a filter on it, whose values are still to give, or a column, which leaves its axis with its
 * precision if it comes from one, into a new filter. Numbers and dates start on `BETWEEN`, the other
 * columns on the list of their values, and the other measures on `=`.
 *
 * @param config Configuration.
 * @param column Index of the column.
 * @param index Position of the filter.
 * @param from Place of the moved element, `null` for a new element.
 * @param types Type of each column of the result.
 */
function placeFilter(
  config: PivotConfig,
  column: number,
  index: number,
  from: ZonePlace | null,
  types: ColumnTypeInfo[],
): PivotConfig {
  const insert = (base: PivotConfig, filter: PivotFilter, at: number) => ({
    ...base,
    filters: [...base.filters.slice(0, at), filter, ...base.filters.slice(at)],
  })
  const moved = from?.zone === 'filters' ? config.filters[from.index] : undefined
  if (from !== null && moved !== undefined) {
    return insert(withoutElement(config, from), moved, from.index < index ? index - 1 : index)
  }
  const measure = from?.zone === 'measures' ? config.measures[from.index] : undefined
  if (measure !== undefined) {
    const filter: PivotFilter = {
      kind: 'measure',
      aggregate: measure.aggregate,
      column,
      name: '',
      condition: isRanged(measureFamily(measure, types)) ? 'between' : 'equal',
      operand: '',
      upper: '',
      level_precision: 'none',
    }
    return insert(config, filter, index)
  }
  const level =
    from?.zone === 'rows' || from?.zone === 'columns' ? config[from.zone][from.index] : undefined
  const base = from === null ? config : removeElement(config, from)
  const filter: PivotFilter = {
    kind: 'column',
    column,
    name: '',
    precision: level?.precision ?? 'none',
    condition: isRanged(types[column]?.family) ? 'between' : 'in_list',
    values: [],
    operand: '',
    upper: '',
  }
  return insert(base, filter, index)
}

/**
 * Places a column in a zone, at a position: a new element, whatever the zones already hold, or one
 * moved from another place.
 *
 * A level moved keeps its name, and a measure moved stays the same measure, with its name; a column
 * that becomes a measure takes the aggregate given, and a measure that becomes a level loses its
 * name. A filter that leaves the filters also loses its name; on a measure, its aggregate stays
 * when it becomes a measure, and on a column, its precision when it becomes a level.
 *
 * @param config Configuration.
 * @param column Index of the column.
 * @param aggregate Aggregate of the column if it becomes a measure (`defaultAggregate`).
 * @param to Zone and position of the element; a position beyond the end places it last.
 * @param from Place of the moved element, `null` for a new element.
 * @param types Type of each column of the result, for the condition of a new filter.
 */
export function placeColumn(
  config: PivotConfig,
  column: number,
  aggregate: PivotMeasure['aggregate'],
  to: ZonePlace,
  from: ZonePlace | null,
  types: ColumnTypeInfo[] = [],
): PivotConfig {
  if (to.zone === 'filters') {
    return placeFilter(config, column, to.index, from, types)
  }
  const moved = from?.zone === 'measures' ? config.measures[from.index] : undefined
  const filter = from?.zone === 'filters' ? config.filters[from.index] : undefined
  const level =
    from?.zone === 'rows' || from?.zone === 'columns' ? config[from.zone][from.index] : undefined
  const result = from === null ? config : withoutElement(config, from)
  // Removing an element before the target, in the same zone, shifts the target.
  const index = from?.zone === to.zone && from.index < to.index ? to.index - 1 : to.index
  const insert = <T>(items: T[], item: T) => [...items.slice(0, index), item, ...items.slice(index)]
  const newLevel: PivotLevel = level ?? {
    column,
    name: '',
    precision: filter?.kind === 'column' ? filter.precision : 'none',
  }
  return withFollowedTargets(config, {
    ...result,
    rows: to.zone === 'rows' ? insert(result.rows, newLevel) : result.rows,
    columns: to.zone === 'columns' ? insert(result.columns, newLevel) : result.columns,
    measures:
      to.zone === 'measures'
        ? insert(
            result.measures,
            moved ?? {
              aggregate: filter?.kind === 'measure' ? filter.aggregate : aggregate,
              column,
              name: '',
            },
          )
        : result.measures,
  })
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
 * Changes the precision of a level, which keeps its name and the filters that read it.
 *
 * @param config Configuration.
 * @param place Place of the level.
 * @param precision New precision.
 */
export function withPrecision(
  config: PivotConfig,
  place: ZonePlace & { zone: 'rows' | 'columns' },
  precision: DatePrecision,
): PivotConfig {
  const changed = config[place.zone][place.index]
  return withFollowedTargets(config, {
    ...config,
    [place.zone]: config[place.zone].map((level, index) =>
      index === place.index ? { ...level, precision } : level,
    ),
    filters: config.filters.map((filter) =>
      filter.kind === 'measure' &&
      filter.level === changed?.column &&
      filter.level_precision === changed?.precision
        ? { ...filter, level_precision: precision }
        : filter,
    ),
  })
}

/**
 * Replaces a filter.
 *
 * @param config Configuration.
 * @param index Position of the filter.
 * @param filter New filter.
 */
export function withFilter(config: PivotConfig, index: number, filter: PivotFilter): PivotConfig {
  return {
    ...config,
    filters: config.filters.map((item, position) => (position === index ? filter : item)),
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
 * @param types Type of each column of the result, for the literals of a filter.
 */
export function withName(
  config: PivotConfig,
  place: ZonePlace,
  name: string,
  names: string[],
  types: ColumnTypeInfo[] = [],
): PivotConfig {
  const trimmed = name.trim()
  const kept = trimmed === elementExpression(config, place, names, types) ? '' : trimmed
  const rename = <T extends { name: string }>(items: T[]) =>
    items.map((item, index) => (index === place.index ? { ...item, name: kept } : item))
  return {
    ...config,
    rows: place.zone === 'rows' ? rename(config.rows) : config.rows,
    columns: place.zone === 'columns' ? rename(config.columns) : config.columns,
    measures: place.zone === 'measures' ? rename(config.measures) : config.measures,
    filters: place.zone === 'filters' ? rename(config.filters) : config.filters,
  }
}

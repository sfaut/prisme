/**
 * Groups of settings of the menu of a filter of the Pivoteur (`PivotSections.vue`, F-19, F-21,
 * F-44). A filter on a measure starts with the group "Filtre": its aggregate and the level it
 * reads, each a line with its value and its submenu of choices. Then come the precision of its
 * dates, its condition as radio choices, and the line of what the condition compares to: the
 * value, the bounds (`PivotFilterOperandSubmenu.vue`) or the values checked
 * (`PivotFilterListSubmenu.vue`). A choice that brings new settings keeps the menu open.
 * Changing the precision empties the values: they have the form of the former one.
 */

import { Columns3, Layers, ListChecks, Rows3, TextCursorInput } from '@lucide/vue'
import type { Component } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { Cell } from '@/api/querySession'
import type { MenuGroup, MenuItem } from '@/components/ui/menuItem'

import { AGGREGATES, CONDITIONS, PRECISION_ICONS } from './pivotChoices'
import {
  type DatePrecision,
  type PivotColumnFilter,
  type PivotCondition,
  type PivotConfig,
  type PivotFilter,
  type PivotLevel,
  type PivotMeasureFilter,
  addsNumbers,
  chosenLevel,
  elementLabel,
  filterable,
  keptPrecision,
  measureExpression,
  measurePrecision,
  operandCount,
  precisionChoices,
  summable,
  takesPrecision,
} from './pivotConfig'
import PivotFilterListSubmenu from './PivotFilterListSubmenu.vue'
import PivotFilterOperandSubmenu from './PivotFilterOperandSubmenu.vue'

/**
 * What a filter needs besides itself to build its menu.
 */
export interface FilterMenuContext {
  /**
   * Configuration, for the levels a filter on a measure can read.
   */
  config: PivotConfig
  /**
   * Names of the columns of the result.
   */
  names: string[]
  /**
   * Type of each column of the result.
   */
  types: ColumnTypeInfo[]
  /**
   * Reads the distinct values of a column, or the periods of a precision of its dates.
   */
  readValues: (column: number, precision: DatePrecision) => Promise<Cell[]>
  /**
   * Are the rows of the result still being received?
   */
  receiving: boolean
}

/**
 * Changes some settings of a filter, read when it is called: a submenu applies what is typed when
 * it closes.
 */
export type FilterChange = (
  changes: Partial<PivotColumnFilter> | Partial<PivotMeasureFilter>,
) => void

/**
 * What a condition compares to: a list of values, one value, two bounds, or nothing.
 *
 * @param condition Condition.
 */
function operandKind(condition: PivotCondition): 'list' | 'one' | 'two' | 'none' {
  if (condition === 'in_list' || condition === 'not_in_list') {
    return 'list'
  }
  return (['none', 'one', 'two'] as const)[operandCount(condition)]
}

/**
 * Label of the line of what the condition of a filter compares to, which opens its submenu; none
 * for `IS NULL` and `IS NOT NULL`. The drop of a filter opens its menu with this submenu open.
 *
 * @param filter Filter.
 */
export function operandLine(filter: PivotFilter): string | undefined {
  return { list: 'Valeurs cochées', one: 'Valeur', two: 'Bornes', none: undefined }[
    operandKind(filter.condition)
  ]
}

/**
 * Value of the line of what a filter compares to: the number of values checked, the value, or the
 * bounds, each one "…" until it is typed.
 *
 * @param filter Filter.
 */
export function operandText(filter: PivotFilter): string {
  switch (operandKind(filter.condition)) {
    case 'list': {
      const count = filter.kind === 'column' ? filter.values.length : 0
      return count === 0 ? 'Aucune' : count.toLocaleString('fr-FR')
    }
    case 'two':
      return filter.operand === '' && filter.upper === ''
        ? 'À saisir'
        : `${filter.operand || '…'} – ${filter.upper || '…'}`
    default:
      return filter.operand || 'À saisir'
  }
}

/**
 * Value of the choice of a level: its column and its precision.
 *
 * @param level Level.
 */
function levelValue({ column, precision }: Pick<PivotLevel, 'column' | 'precision'>): string {
  return `${column}:${precision}`
}

/**
 * Group "Filtre" of a filter on a measure: its aggregate, among those a filter reads
 * (`filterable`), and the level it reads, the finest one by default, then those of the two axes,
 * the same level placed twice being one choice.
 *
 * @param filter Filter on a measure.
 * @param context What the menu needs besides the filter.
 * @param change Changes some settings of the filter.
 */
function measureGroup(
  filter: PivotMeasureFilter,
  { config, names, types }: FilterMenuContext,
  change: FilterChange,
): MenuGroup {
  const family = types[filter.column]?.family
  const numbers = summable(family)
  const dates = precisionChoices(family).length > 0
  const aggregates: MenuItem[] = AGGREGATES.filter(({ aggregate }) => filterable(aggregate)).map(
    ({ aggregate, icon }) => {
      const refused = addsNumbers(aggregate) && !numbers
      const { column, precision } = filter
      return {
        label: measureExpression({ aggregate, column, precision }, names),
        icon,
        checked: aggregate === filter.aggregate,
        disabled: refused,
        hint: refused ? 'Demande une colonne de nombres' : undefined,
        // The precision of the dates appears with the aggregate.
        keepOpen: dates && takesPrecision(aggregate) && !takesPrecision(filter.aggregate),
        onSelect: () =>
          change({ aggregate, precision: keptPrecision(aggregate, filter.precision) }),
      }
    },
  )
  const chosen = chosenLevel(config, filter)
  const seen = new Set<string>()
  const levels: MenuItem[] = [
    {
      label: 'Le niveau le plus fin',
      icon: Layers,
      checked: chosen === undefined,
      onSelect: () => change({ level: undefined, level_precision: 'none' }),
    },
  ]
  for (const [zone, icon] of [
    ['rows', Rows3],
    ['columns', Columns3],
  ] as const) {
    config[zone].forEach((level, index) => {
      const value = levelValue(level)
      if (!seen.has(value)) {
        seen.add(value)
        levels.push({
          label: elementLabel(config, { zone, index }, names),
          icon: icon as Component,
          checked: chosen !== undefined && levelValue(chosen) === value,
          onSelect: () => change({ level: level.column, level_precision: level.precision }),
        })
      }
    })
  }
  const current = AGGREGATES.find(({ aggregate }) => aggregate === filter.aggregate)
  return {
    label: 'Filtre',
    items: [
      {
        label: 'Agrégat',
        icon: current?.icon ?? Layers,
        value: measureExpression(filter, names),
        groups: [{ label: 'Agrégat', items: aggregates }],
      },
      {
        label: 'Niveau',
        icon: Layers,
        value: levels.find((item) => item.checked)?.label,
        groups: [{ label: 'Niveau', items: levels }],
      },
    ],
  }
}

/**
 * Choices of the precision of the dates of a filter, the current one checked: none for a column
 * that holds no dates, nor for a measure whose aggregate takes none. A new precision empties what
 * the filter compares to.
 *
 * @param filter Filter.
 * @param types Type of each column of the result.
 * @param change Changes some settings of the filter.
 */
function precisionItems(
  filter: PivotFilter,
  types: ColumnTypeInfo[],
  change: FilterChange,
): MenuItem[] {
  if (filter.kind === 'measure' && !takesPrecision(filter.aggregate)) {
    return []
  }
  const current = filter.kind === 'column' ? filter.precision : measurePrecision(filter)
  return precisionChoices(types[filter.column]?.family).map(({ precision, label }) => ({
    label,
    icon: PRECISION_ICONS[label],
    checked: precision === current,
    onSelect: () => {
      if (precision !== current) {
        change(
          filter.kind === 'column'
            ? { precision, values: [], operand: '', upper: '' }
            : { precision: keptPrecision(filter.aggregate, precision), operand: '', upper: '' },
        )
      }
    },
  }))
}

/**
 * Line of what the condition of a filter compares to, with its submenu: the fields of the value
 * or of the bounds, or the list of the values of the column.
 *
 * @param filter Filter.
 * @param context What the menu needs besides the filter.
 * @param change Changes some settings of the filter.
 */
function operandItem(
  filter: PivotFilter,
  { names, types, readValues, receiving }: FilterMenuContext,
  change: FilterChange,
): MenuItem | undefined {
  const label = operandLine(filter)
  if (label === undefined) {
    return undefined
  }
  if (filter.kind === 'column' && operandKind(filter.condition) === 'list') {
    return {
      label,
      icon: ListChecks,
      value: operandText(filter),
      panel: {
        component: PivotFilterListSubmenu,
        props: {
          label: names[filter.column] ?? '?',
          read: () => readValues(filter.column, filter.precision),
          offersNull: filter.condition === 'in_list',
          partial: receiving,
          kept: filter.values,
          onChange: (values: Cell[]) => change({ values }),
        },
      },
    }
  }
  return {
    label,
    icon: TextCursorInput,
    value: operandText(filter),
    panel: {
      component: PivotFilterOperandSubmenu,
      props: {
        filter,
        types,
        onChange: (changes: Pick<PivotFilter, 'operand' | 'upper'>) => change(changes),
      },
    },
  }
}

/**
 * Groups of the settings of the menu of a filter, before those of the element.
 *
 * @param filter Filter.
 * @param context What the menu needs besides the filter.
 * @param change Changes some settings of the filter.
 */
export function filterGroups(
  filter: PivotFilter,
  context: FilterMenuContext,
  change: FilterChange,
): MenuGroup[] {
  const groups: MenuGroup[] = []
  if (filter.kind === 'measure') {
    groups.push(measureGroup(filter, context, change))
  }
  const precisions = precisionItems(filter, context.types, change)
  if (precisions.length > 0) {
    groups.push({ label: 'Précision', items: precisions })
  }
  const current = operandKind(filter.condition)
  groups.push({
    label: 'Condition',
    items: CONDITIONS.filter(
      ({ condition }) => filter.kind === 'column' || operandKind(condition) !== 'list',
    ).map(({ condition, label, icon }) => ({
      label,
      icon,
      checked: condition === filter.condition,
      // A condition that compares to something else shows its line at once.
      keepOpen: operandKind(condition) !== 'none' && operandKind(condition) !== current,
      onSelect: () => change({ condition }),
    })),
  })
  const operand = operandItem(filter, context, change)
  if (operand !== undefined) {
    groups.push({ label: operand.label, items: [operand] })
  }
  return groups
}

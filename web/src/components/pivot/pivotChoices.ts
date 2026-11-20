/**
 * Choices the menus of the elements of the Pivoteur share, with their icon (`PivotSections.vue`,
 * `pivotMenus.ts`, `pivotFilterMenu.ts`): aggregates, precisions, conditions of a filter, display of
 * a measure as a share of a total and its digits, usual separators of a `GROUP_CONCAT`.
 */

import {
  ArrowDownToLine,
  ArrowUpToLine,
  Asterisk,
  Atom,
  Calendar,
  Calendar1,
  CalendarClock,
  CalendarDays,
  CalendarFold,
  CalendarRange,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  Circle,
  CircleDot,
  Clock,
  Clock3,
  Columns3,
  Combine,
  CornerDownLeft,
  DecimalsArrowRight,
  Divide,
  Equal,
  EqualNot,
  FingerprintPattern,
  FoldHorizontal,
  Hash,
  Hourglass,
  ListChecks,
  ListX,
  Rows3,
  Sigma,
  SquareDivide,
  SquareSigma,
  TextCursorInput,
  Timer,
  UnfoldHorizontal,
} from '@lucide/vue'
import type { Component } from 'vue'

import type { PivotShare } from '@/api/generated/PivotShare'

import type { PivotCondition, PrecisionLabel } from './pivotConfig'
import type { PivotMeasure } from './pivotEngine'

/**
 * Aggregates a measure can take, with their icon.
 */
export const AGGREGATES: { aggregate: PivotMeasure['aggregate']; icon: Component }[] = [
  { aggregate: 'count_star', icon: Asterisk },
  { aggregate: 'count', icon: Hash },
  { aggregate: 'count_distinct', icon: FingerprintPattern },
  { aggregate: 'sum', icon: Sigma },
  { aggregate: 'sum_distinct', icon: SquareSigma },
  { aggregate: 'avg', icon: Divide },
  { aggregate: 'avg_distinct', icon: SquareDivide },
  { aggregate: 'min', icon: ArrowDownToLine },
  { aggregate: 'max', icon: ArrowUpToLine },
  { aggregate: 'group_concat', icon: Combine },
]

/**
 * Icon of each precision.
 */
export const PRECISION_ICONS: Record<PrecisionLabel, Component> = {
  Année: Calendar,
  Trimestre: CalendarRange,
  Mois: CalendarDays,
  Semaine: CalendarFold,
  Date: Calendar1,
  Horodatage: CalendarClock,
  'Horodatage (hh)': Clock3,
  'Horodatage (hh:mm)': Clock,
  'Horodatage (hh:mm:ss)': Timer,
  'Horodatage (hh:mm:ss.ms)': Hourglass,
  'Horodatage (hh:mm:ss.µs)': Atom,
}

/**
 * Conditions of a filter, with their icon. A filter on a measure has no list.
 */
export const CONDITIONS: { condition: PivotCondition; label: string; icon: Component }[] = [
  { condition: 'in_list', label: 'Dans la liste', icon: ListChecks },
  { condition: 'not_in_list', label: 'Hors de la liste', icon: ListX },
  { condition: 'equal', label: 'Égal à', icon: Equal },
  { condition: 'not_equal', label: 'Différent de', icon: EqualNot },
  { condition: 'less', label: 'Inférieur à', icon: ChevronLeft },
  { condition: 'less_or_equal', label: 'Inférieur ou égal à', icon: ChevronsLeft },
  { condition: 'greater', label: 'Supérieur à', icon: ChevronRight },
  { condition: 'greater_or_equal', label: 'Supérieur ou égal à', icon: ChevronsRight },
  { condition: 'between', label: 'Entre', icon: FoldHorizontal },
  { condition: 'not_between', label: 'N’est pas entre', icon: UnfoldHorizontal },
  { condition: 'is_null', label: 'Est NULL', icon: Circle },
  { condition: 'is_not_null', label: 'N’est pas NULL', icon: CircleDot },
]

/**
 * How a count, a sum or an average shows its values, with their icon: as they are (`null`), or as
 * the share of the total of their line or of their column.
 */
export const SHOW_AS: { of: PivotShare['of'] | null; label: string; icon: Component }[] = [
  { of: null, label: 'Valeur', icon: Equal },
  { of: 'row', label: 'Part du total de la ligne', icon: Rows3 },
  { of: 'column', label: 'Part du total de la colonne', icon: Columns3 },
]

/**
 * Digits after the point a share can take, 0 by default.
 */
export const SHARE_DECIMALS = [0, 1, 2, 3, 4, 5, 6]

/**
 * Icon of the choices of the digits of a share.
 */
export const DECIMALS_ICON: Component = DecimalsArrowRight

/**
 * Usual separators of a `GROUP_CONCAT`, offered beside its field, with their icon.
 */
export const SEPARATORS: { separator: string; label: string; icon: Component }[] = [
  { separator: ', ', label: 'Virgule', icon: TextCursorInput },
  { separator: '; ', label: 'Point-virgule', icon: TextCursorInput },
  { separator: ' | ', label: 'Barre', icon: TextCursorInput },
  { separator: '\n', label: 'Retour à la ligne (LF)', icon: CornerDownLeft },
  { separator: '\r\n', label: 'Retour à la ligne (CR LF)', icon: CornerDownLeft },
]

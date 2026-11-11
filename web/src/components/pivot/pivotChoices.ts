/**
 * Choices the settings of the Pivoteur share, with their icon: the menu of a measure and of a level
 * (`PivotSections.vue`), and the panel of a filter (`PivotFilterPopover.vue`); the display of a measure
 * as a share of a total and its digits.
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
  Clock,
  Clock3,
  Columns3,
  Combine,
  DecimalsArrowRight,
  Divide,
  Equal,
  FingerprintPattern,
  Hash,
  Hourglass,
  Rows3,
  Sigma,
  SquareDivide,
  SquareSigma,
  Timer,
} from '@lucide/vue'
import type { Component } from 'vue'

import type { PivotShare } from '@/api/generated/PivotShare'

import type { PrecisionLabel } from './pivotConfig'
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

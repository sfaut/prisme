/**
 * Choices the settings of the Pivoteur share, with their icon: the menu of a measure and of a level
 * (`PivotZones.vue`), and the panel of a filter (`PivotFilterPanel.vue`).
 */

import {
  ArrowDownToLine,
  ArrowUpToLine,
  Asterisk,
  Calendar,
  Calendar1,
  CalendarClock,
  CalendarDays,
  CalendarFold,
  CalendarRange,
  Divide,
  FingerprintPattern,
  Hash,
  Sigma,
  SquareDivide,
  SquareSigma,
} from '@lucide/vue'
import type { Component } from 'vue'

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
  'Date et heure': CalendarClock,
}

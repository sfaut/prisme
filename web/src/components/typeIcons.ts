/**
 * Icône de chaque famille de types de colonne, affichée devant le nom des
 * colonnes dans l'en-tête de la grille de résultats (à la manière de
 * DuckDB UI). Les icônes sont neutres : même couleur pour toutes les
 * familles ; le nom natif du type, affiché à côté, donne le détail.
 */

import {
  Binary,
  Braces,
  Calendar,
  CalendarClock,
  CalendarDays,
  CaseSensitive,
  CircleDashed,
  Clock,
  Hash,
  List,
  ListChecks,
  Shapes,
  ToggleRight,
} from 'lucide-vue-next'
import type { Component } from 'vue'

import type { TypeFamily } from '@/api/generated/TypeFamily'

/**
 * Icône Lucide de chaque famille de types.
 */
export const TYPE_ICONS: Record<TypeFamily, Component> = {
  integer: Hash,
  decimal: Hash,
  float: Hash,
  text: CaseSensitive,
  binary: Binary,
  date: Calendar,
  time: Clock,
  datetime: CalendarClock,
  year: CalendarDays,
  json: Braces,
  bit: ToggleRight,
  enum: List,
  set: ListChecks,
  geometry: Shapes,
  other: CircleDashed,
}

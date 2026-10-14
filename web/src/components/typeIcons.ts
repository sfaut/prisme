/**
 * Icône de chaque famille de types de colonne, affichée devant le nom des
 * colonnes dans l'en-tête de la grille de résultats (à la manière de
 * DuckDB UI). Les icônes sont neutres : même couleur pour toutes les
 * familles ; le nom natif du type, au survol de l'en-tête, donne le détail.
 *
 * Une colonne de texte dont les valeurs sont des documents JSON ou XML
 * (`contentKind.ts`) prend l'icône de son contenu.
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
  CodeXml,
  Hash,
  List,
  ListChecks,
  Shapes,
  ToggleRight,
} from 'lucide-vue-next'
import type { Component } from 'vue'

import type { ColumnTypeInfo } from '@/api/generated/ColumnTypeInfo'
import type { TypeFamily } from '@/api/generated/TypeFamily'

import type { ContentKind } from './contentKind'

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

/**
 * Icône d'une colonne : celle de son contenu pour une colonne de texte qui
 * contient du JSON ou du XML, sinon celle de la famille de son type.
 *
 * @param type Type de la colonne.
 * @param content Contenu détecté, `null` si aucun format reconnu.
 */
export function columnIcon(type: ColumnTypeInfo, content: ContentKind | null): Component {
  if (type.family === 'text' && content === 'json') {
    return Braces
  }
  if (type.family === 'text' && content === 'xml') {
    return CodeXml
  }
  return TYPE_ICONS[type.family]
}

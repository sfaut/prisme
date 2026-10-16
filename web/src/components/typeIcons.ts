/**
 * Icon of each family of column types, shown before the column names in the
 * header of the result grid (in the manner of DuckDB UI). The icons are
 * neutral: same color for all the families; the native name of the type, on
 * hovering the header, gives the detail.
 *
 * As in DuckDB UI, numbers are told apart by a mark: `123` for integers, `#`
 * for floats, `.00` for decimals. As Lucide has no icon for `123` and `.00`,
 * these two marks are text (a string instead of a component).
 *
 * A text column whose values are JSON or XML documents, or web addresses
 * (`contentKind.ts`), takes the icon of its content.
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
  Link,
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
 * Mark of a type: a Lucide icon, or a short text.
 */
export type TypeMark = Component | string

/**
 * Mark of each family of types.
 */
export const TYPE_ICONS: Record<TypeFamily, TypeMark> = {
  integer: '123',
  decimal: '.00',
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
 * Mark of a column: the icon of its content for a text column that holds
 * JSON or XML, otherwise the mark of the family of its type.
 *
 * @param type Type of the column.
 * @param content Detected content, `null` if no format is recognized.
 */
export function columnIcon(type: ColumnTypeInfo, content: ContentKind | null): TypeMark {
  if (type.family === 'text' && content === 'json') {
    return Braces
  }
  if (type.family === 'text' && content === 'xml') {
    return CodeXml
  }
  if (type.family === 'text' && content === 'url') {
    return Link
  }
  return TYPE_ICONS[type.family]
}

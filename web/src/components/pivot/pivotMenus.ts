/**
 * Groups of settings of the menu of an element of the Pivoteur (`PivotSections.vue`) that hold
 * values and submenus: the settings of a `GROUP_CONCAT` (group "Concaténation") and the display
 * of a count, a sum or an average (group "Affichage"). Each setting shows its current value
 * beside its choice; a setting to type opens a submenu of fields (`MenuPanel`).
 */

import { Brackets, FingerprintPattern, ListOrdered, TextCursorInput } from '@lucide/vue'

import type { PivotShare } from '@/api/generated/PivotShare'
import type { MenuGroup, MenuItem } from '@/components/ui/menuItem'

import { DECIMALS_ICON, SHARE_DECIMALS, SHOW_AS } from './pivotChoices'
import { type PivotConcat, separatorLiteral } from './pivotConfig'
import PivotConcatOrderSubmenu from './PivotConcatOrderSubmenu.vue'
import PivotConcatSeparatorSubmenu from './PivotConcatSeparatorSubmenu.vue'
import PivotConcatWindowSubmenu from './PivotConcatWindowSubmenu.vue'

/**
 * Order of the values as its choice shows it: its keys, a descending one marked by an arrow, or
 * the order of the rows of the result without any.
 *
 * @param concat Settings of the measure.
 * @param names Names of the columns of the result.
 */
export function orderText(concat: PivotConcat, names: string[]): string {
  const keys = concat.order ?? []
  return keys.length === 0
    ? 'Ordre du résultat'
    : keys
        .map(({ column, descending }) => `${names[column] ?? '?'}${descending ? ' ↓' : ''}`)
        .join(', ')
}

/**
 * Window of the values as its choice shows it: all of them, `LIMIT`, or `LIMIT / OFFSET`.
 *
 * @param concat Settings of the measure.
 */
export function windowText({ limit, offset }: PivotConcat): string {
  if (limit === undefined) {
    return 'Toutes'
  }
  return offset ? `${limit} / ${offset}` : String(limit)
}

/**
 * Group "Concaténation" of the menu of a `GROUP_CONCAT`: `DISTINCT` as a checkbox, then the order,
 * the separator and the window, each with its value and its submenu.
 *
 * @param concat Settings of the measure.
 * @param column Column of the measure.
 * @param names Names of the columns of the result.
 * @param change
 *     Changes some settings; reads those of the measure when it is called, a submenu applying what
 *     is typed when it closes.
 */
export function concatGroup(
  concat: PivotConcat,
  column: number,
  names: string[],
  change: (changes: Partial<PivotConcat>) => void,
): MenuGroup {
  return {
    label: 'Concaténation',
    items: [
      {
        label: 'Valeurs distinctes (DISTINCT)',
        icon: FingerprintPattern,
        ticked: concat.distinct === true,
        onSelect: () => change({ distinct: concat.distinct !== true }),
      },
      {
        label: 'Ordre (ORDER BY)',
        icon: ListOrdered,
        value: orderText(concat, names),
        panel: {
          component: PivotConcatOrderSubmenu,
          props: {
            order: concat.order ?? [],
            names,
            column,
            onChange: (order: PivotConcat['order']) => change({ order }),
          },
        },
      },
      {
        label: 'Séparateur (SEPARATOR)',
        icon: TextCursorInput,
        value: separatorLiteral(concat.separator),
        panel: {
          component: PivotConcatSeparatorSubmenu,
          props: {
            separator: concat.separator,
            onChange: (separator: string) => change({ separator }),
          },
        },
      },
      {
        label: 'Limite (LIMIT / OFFSET)',
        icon: Brackets,
        value: windowText(concat),
        panel: {
          component: PivotConcatWindowSubmenu,
          props: {
            limit: concat.limit,
            offset: concat.offset,
            onChange: (window: Pick<PivotConcat, 'limit' | 'offset'>) => change(window),
          },
        },
      },
    ],
  }
}

/**
 * Group "Affichage" of the menu of a count, a sum or an average, the current choice checked: its
 * values as they are, or their share of the total of their line or of their column. Choosing a
 * share keeps the menu open, which then shows the line of its digits after the point, with their
 * submenu.
 *
 * @param share Share the measure shows, absent for its values as they are.
 * @param change Shows the values as they are (`undefined`), or a share.
 */
export function shareGroup(
  share: PivotShare | undefined,
  change: (share: PivotShare | undefined) => void,
): MenuGroup {
  const items: MenuItem[] = SHOW_AS.map(({ of, label, icon }) => ({
    label,
    icon,
    checked: (share?.of ?? null) === of,
    keepOpen: share === undefined && of !== null,
    onSelect: () => change(of === null ? undefined : { of, decimals: share?.decimals ?? 0 }),
  }))
  if (share !== undefined) {
    items.push({
      label: 'Décimales',
      icon: DECIMALS_ICON,
      value: share.decimals.toLocaleString('fr-FR'),
      groups: [
        {
          label: 'Décimales',
          items: SHARE_DECIMALS.map((decimals) => ({
            label: decimals.toLocaleString('fr-FR'),
            icon: DECIMALS_ICON,
            checked: decimals === share.decimals,
            onSelect: () => change({ ...share, decimals }),
          })),
        },
      ],
    })
  }
  return { label: 'Affichage', items }
}

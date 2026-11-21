/**
 * Tooltips of the interface: the `v-tooltip` directive gives an element its
 * tooltip, and a single bubble (`UiTooltip.vue`, mounted once by `App.vue`)
 * shows the tooltip of the element under the pointer or with the keyboard
 * focus.
 *
 * One bubble for the whole page, and a directive rather than a wrapping
 * component: the directive adds nothing to the DOM nor to the layout, so it
 * fits any element (table cell, grid cell, button, label), and a grid of
 * thousands of cells costs one attribute per cell.
 *
 * A tooltip has up to three parts, each with its own style in the bubble:
 * its title, first, what the element does ("Arrêter la requête") or the
 * value it shows in full; its keyboard shortcut, on the line of the title,
 * after it; its description, on the next line, subtle, when the title needs
 * one ("Les lignes reçues restent"). A string is a title alone.
 *
 * The bubble shows above its element; `v-tooltip:right` puts it on the
 * right, for an element on the left edge of the window (activity bar), whose
 * neighbours above and below share the same kind of tooltip.
 */

import { type Directive, reactive } from 'vue'

/**
 * Attribute that holds the title of an element's tooltip.
 */
export const TOOLTIP_ATTRIBUTE = 'data-tooltip'

/**
 * Attribute that holds the description of an element's tooltip.
 */
export const TOOLTIP_DESCRIPTION_ATTRIBUTE = 'data-tooltip-description'

/**
 * Attribute that holds the keyboard shortcut of an element's tooltip.
 */
export const TOOLTIP_SHORTCUT_ATTRIBUTE = 'data-tooltip-shortcut'

/**
 * Attribute that holds the side of an element's tooltip, when it is not the
 * top.
 */
export const TOOLTIP_SIDE_ATTRIBUTE = 'data-tooltip-side'

/**
 * Parts of a tooltip.
 */
export interface Tooltip {
  /**
   * What the element does, starting with a verb ("Exécuter la requête sous
   * le curseur"), or the value it shows in full.
   */
  title: string
  /**
   * What the title needs to be understood, on the next line, subtle; absent
   * or empty, none.
   */
  description?: string
  /**
   * Keyboard shortcut of the action ("Ctrl-Entrée"); absent or empty, none.
   */
  shortcut?: string
}

/**
 * Value of `v-tooltip`: a title alone, or the parts of the tooltip; empty or
 * absent, no tooltip.
 */
export type TooltipValue = string | Tooltip | null | undefined

/**
 * Side of the element where the bubble shows.
 */
export type TooltipSide = 'top' | 'right'

/**
 * Side of an element's tooltip.
 *
 * @param element Element of the tooltip.
 */
export function tooltipSideOf(element: HTMLElement): TooltipSide {
  return element.getAttribute(TOOLTIP_SIDE_ATTRIBUTE) === 'right' ? 'right' : 'top'
}

/**
 * Characters of a title at most: a cell can hold a whole document, the
 * bubble only shows its beginning.
 */
const MAX_LENGTH = 1000

/**
 * Element whose tooltip is shown, `null` when none is, the parts of its
 * tooltip (empty when absent) and its side.
 */
export const shownTooltip = reactive<{
  element: HTMLElement | null
  title: string
  description: string
  shortcut: string
  side: TooltipSide
}>({
  element: null,
  title: '',
  description: '',
  shortcut: '',
  side: 'top',
})

/**
 * Parts of a tooltip from the value of the directive, empty when absent; no
 * description nor shortcut without a title.
 *
 * @param value Value of `v-tooltip`.
 */
export function tooltipParts(value: TooltipValue): Required<Tooltip> {
  const parts = typeof value === 'string' ? { title: value } : (value ?? { title: '' })
  const title =
    parts.title.length > MAX_LENGTH ? `${parts.title.slice(0, MAX_LENGTH)}…` : parts.title
  return title === ''
    ? { title, description: '', shortcut: '' }
    : { title, description: parts.description ?? '', shortcut: parts.shortcut ?? '' }
}

/**
 * Sets an attribute of an element, or removes it when its value is empty.
 *
 * @param element Element of the tooltip.
 * @param name Name of the attribute.
 * @param value Value of the attribute, empty to remove it.
 */
function setOrRemove(element: HTMLElement, name: string, value: string): void {
  if (value === '') {
    element.removeAttribute(name)
  } else {
    element.setAttribute(name, value)
  }
}

/**
 * Gives an element its tooltip, or takes it away.
 *
 * @param element Element of the tooltip.
 * @param value Title of the tooltip, or its parts; empty or absent, no
 *     tooltip.
 * @param side Side of the bubble, `right` or the top when absent.
 */
function mark(element: HTMLElement, value: TooltipValue, side?: string): void {
  const parts = tooltipParts(value)
  setOrRemove(element, TOOLTIP_ATTRIBUTE, parts.title)
  setOrRemove(element, TOOLTIP_DESCRIPTION_ATTRIBUTE, parts.description)
  setOrRemove(element, TOOLTIP_SHORTCUT_ATTRIBUTE, parts.shortcut)
  setOrRemove(element, TOOLTIP_SIDE_ATTRIBUTE, side === 'right' ? side : '')
  if (shownTooltip.element === element) {
    Object.assign(shownTooltip, parts)
    if (parts.title === '') {
      shownTooltip.element = null
    }
  }
}

/**
 * `v-tooltip="'Title'"` or `v-tooltip="{ title, description, shortcut }"`:
 * tooltip of the element, empty or absent for none, above it;
 * `v-tooltip:right="…"`, on its right. A tooltip that changes while it shows
 * (a button that turns into a stop button) changes in the bubble too.
 */
export const vTooltip: Directive<HTMLElement, TooltipValue> = {
  mounted: (element, { value, arg }) => mark(element, value, arg),
  updated: (element, { value, arg }) => mark(element, value, arg),
  beforeUnmount: (element) => {
    if (shownTooltip.element === element) {
      shownTooltip.element = null
    }
  },
}

/**
 * Tooltips of the interface: the `v-tooltip` directive gives an element its
 * text, and a single bubble (`UiTooltip.vue`, mounted once by `App.vue`)
 * shows the text of the element under the pointer or with the keyboard focus.
 *
 * One bubble for the whole page, and a directive rather than a wrapping
 * component: the directive adds nothing to the DOM nor to the layout, so it
 * fits any element (table cell, grid cell, button, label), and a grid of
 * thousands of cells costs one attribute per cell.
 *
 * The bubble shows above its element; `v-tooltip:right` puts it on the
 * right, for an element on the left edge of the window (activity bar), whose
 * neighbours above and below share the same kind of tooltip.
 */

import { type Directive, reactive } from 'vue'

/**
 * Attribute that holds the text of an element's tooltip.
 */
export const TOOLTIP_ATTRIBUTE = 'data-tooltip'

/**
 * Attribute that holds the side of an element's tooltip, when it is not the
 * top.
 */
export const TOOLTIP_SIDE_ATTRIBUTE = 'data-tooltip-side'

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
 * Characters of a tooltip at most: a cell can hold a whole document, the
 * bubble only shows its beginning.
 */
const MAX_LENGTH = 1000

/**
 * Element whose tooltip is shown, `null` when none is, its text and its
 * side.
 */
export const shownTooltip = reactive<{
  element: HTMLElement | null
  text: string
  side: TooltipSide
}>({
  element: null,
  text: '',
  side: 'top',
})

/**
 * Gives an element the text of its tooltip, or takes it away.
 *
 * @param element Element of the tooltip.
 * @param text Text of the tooltip; empty or absent, no tooltip.
 * @param side Side of the bubble, `right` or the top when absent.
 */
function mark(element: HTMLElement, text: string | null | undefined, side?: string): void {
  const shown = text ? (text.length > MAX_LENGTH ? `${text.slice(0, MAX_LENGTH)}…` : text) : ''
  if (shown === '') {
    element.removeAttribute(TOOLTIP_ATTRIBUTE)
  } else {
    element.setAttribute(TOOLTIP_ATTRIBUTE, shown)
  }
  if (side === 'right') {
    element.setAttribute(TOOLTIP_SIDE_ATTRIBUTE, side)
  } else {
    element.removeAttribute(TOOLTIP_SIDE_ATTRIBUTE)
  }
  if (shownTooltip.element === element) {
    shownTooltip.text = shown
    if (shown === '') {
      shownTooltip.element = null
    }
  }
}

/**
 * `v-tooltip="text"`: tooltip of the element, empty or absent for none, above
 * it; `v-tooltip:right="text"`, on its right. A text that changes while it
 * shows (a button that turns into a stop button) changes in the bubble too.
 */
export const vTooltip: Directive<HTMLElement, string | null | undefined> = {
  mounted: (element, { value, arg }) => mark(element, value, arg),
  updated: (element, { value, arg }) => mark(element, value, arg),
  beforeUnmount: (element) => {
    if (shownTooltip.element === element) {
      shownTooltip.element = null
    }
  },
}

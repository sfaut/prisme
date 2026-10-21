/**
 * Tooltips of the interface: the `v-tooltip` directive gives an element its
 * text, and a single bubble (`UiTooltip.vue`, mounted once by `App.vue`)
 * shows the text of the element under the pointer or with the keyboard focus.
 *
 * One bubble for the whole page, and a directive rather than a wrapping
 * component: the directive adds nothing to the DOM nor to the layout, so it
 * fits any element (table cell, grid cell, button, label), and a grid of
 * thousands of cells costs one attribute per cell.
 */

import { type Directive, reactive } from 'vue'

/**
 * Attribute that holds the text of an element's tooltip.
 */
export const TOOLTIP_ATTRIBUTE = 'data-tooltip'

/**
 * Characters of a tooltip at most: a cell can hold a whole document, the
 * bubble only shows its beginning.
 */
const MAX_LENGTH = 1000

/**
 * Element whose tooltip is shown, `null` when none is, and its text.
 */
export const shownTooltip = reactive<{ element: HTMLElement | null; text: string }>({
  element: null,
  text: '',
})

/**
 * Gives an element the text of its tooltip, or takes it away.
 *
 * @param element Element of the tooltip.
 * @param text Text of the tooltip; empty or absent, no tooltip.
 */
function mark(element: HTMLElement, text: string | null | undefined): void {
  const shown = text ? (text.length > MAX_LENGTH ? `${text.slice(0, MAX_LENGTH)}…` : text) : ''
  if (shown === '') {
    element.removeAttribute(TOOLTIP_ATTRIBUTE)
  } else {
    element.setAttribute(TOOLTIP_ATTRIBUTE, shown)
  }
  if (shownTooltip.element === element) {
    shownTooltip.text = shown
    if (shown === '') {
      shownTooltip.element = null
    }
  }
}

/**
 * `v-tooltip="text"`: tooltip of the element, empty or absent for none. A
 * text that changes while it shows (a button that turns into a stop button)
 * changes in the bubble too.
 */
export const vTooltip: Directive<HTMLElement, string | null | undefined> = {
  mounted: (element, { value }) => mark(element, value),
  updated: (element, { value }) => mark(element, value),
  beforeUnmount: (element) => {
    if (shownTooltip.element === element) {
      shownTooltip.element = null
    }
  },
}

/**
 * Splitter of Prisme: panels side by side or one above the
 * other, laid out by the browser (flexbox) with the lengths of the theme, and
 * resized by the lines between them (`SplitGroup`, `SplitPanel`,
 * `SplitHandle`).
 *
 * A panel takes its default size (a theme variable), or the size the user gave
 * it, in CSS pixels, as the line left it: the line follows the pointer, and
 * its position is kept as it is at the end of the drag, without rounding. One panel of the group fills the room left. The browser lays the
 * group out on its own, shown or hidden: the code only computes during a drag
 * or a key on a line, from the lengths it measures then. This module holds
 * that computation, without the page.
 */
import { type InjectionKey, inject } from 'vue'

/**
 * Panels side by side (`horizontal`) or one above the other (`vertical`).
 */
export type SplitDirection = 'horizontal' | 'vertical'

/**
 * Panel of a group, as the lines of the group see it.
 */
export interface SplitPanelEntry {
  /**
   * Id of the panel, named by the line that resizes it (`controls`).
   */
  id: () => string
  /**
   * Element of the panel, once mounted.
   */
  element: () => HTMLElement | null
  /**
   * Does the panel fill the room left by the others?
   */
  isFill: () => boolean
  /**
   * Is the panel folded?
   */
  isFolded: () => boolean
  /**
   * Smallest size open (CSS length), absent for none; read from the panel,
   * as a folded panel drops it from its style.
   */
  minSize: () => string | undefined
  /**
   * Folded size (CSS length), absent when the panel does not fold.
   */
  foldedSize: () => string | undefined
  /**
   * Shows a size during a drag, in CSS pixels, or the kept size again
   * (`null`).
   */
  showSize: (size: number | null) => void
  /**
   * Keeps a size, in CSS pixels.
   */
  keepSize: (size: number) => void
  /**
   * Folds or unfolds the panel.
   */
  fold: (folded: boolean) => void
}

/**
 * What a group gives to its panels and its lines.
 */
export interface SplitGroupContext {
  /**
   * Direction of the group.
   */
  direction: () => SplitDirection
  /**
   * Panels of the group, in the order of the page.
   */
  panels: () => SplitPanelEntry[]
  /**
   * Adds a panel to the group.
   */
  register: (panel: SplitPanelEntry) => void
  /**
   * Removes a panel from the group.
   */
  unregister: (panel: SplitPanelEntry) => void
  /**
   * Measures a CSS length (`var(--spacing)`, `var(--rail-size)`) in pixels,
   * at the time of the call.
   */
  measure: (length: string) => number
  /**
   * Room of the group no panel or line takes, in pixels, at the time of the
   * call: what a line takes from when no panel fills the group.
   */
  freeRoom: () => number
}

/**
 * Key under which a group gives its context to its panels and its lines.
 */
export const SPLIT_GROUP: InjectionKey<SplitGroupContext> = Symbol('split-group')

/**
 * Context of the group a panel or a line stands in.
 *
 * @param component Name of the component, for the error.
 * @throws {Error} Outside of a group.
 */
export function useSplitGroup(component: string): SplitGroupContext {
  const group = inject(SPLIT_GROUP)
  if (group === undefined) {
    throw new Error(`${component} outside of a SplitGroup`)
  }
  return group
}

/**
 * Number of steps a key moves a line by: four steps, a `rem`.
 */
export const KEY_STEPS = 4

/**
 * Move of a line, in pixels, beyond any room: towards the largest size of its
 * panel. Small enough for the sums of pixels to stay exact.
 */
export const FAR = 1e7

/**
 * A panel at the start of a drag, its lengths in pixels.
 */
export interface DragPanel {
  /**
   * Size shown.
   */
  size: number
  /**
   * Smallest size, open.
   */
  min: number
  /**
   * Folded size, `null` when the panel does not fold.
   */
  folded: number | null
  /**
   * Is the panel folded?
   */
  isFolded: boolean
}

/**
 * State of a group at the start of a drag, as a line sees it.
 */
export interface DragStart {
  /**
   * Panel the line resizes.
   */
  target: DragPanel
  /**
   * Room the panel that fills can give: its size beyond its smallest one.
   */
  fillRoom: number
}

/**
 * Size and fold of the panel of a line after a move of the line.
 */
export interface DragResult {
  /**
   * Size, in CSS pixels, as the line left it; `null` when folded (the panel
   * keeps the size it unfolds to).
   */
  size: number | null
  /**
   * Is the panel folded?
   */
  folded: boolean
}

/**
 * Small gap under which two lengths in pixels are equal: the sizes measured
 * come back from the layout with fractions.
 */
const EPSILON = 1e-6

/**
 * Size of the panel of a line when the line has moved by `delta` pixels,
 * towards the room of the panel that fills (positive: the panel of the line
 * grows). The size follows the move exactly, without rounding.
 *
 * Below its smallest size, the panel folds past half the way to its folded
 * size, and stops at its smallest size before. It takes its room from the
 * panel that fills, down to the smallest size of that one, and stops there.
 *
 * @param start State of the group at the start of the drag.
 * @param delta Move of the line, in pixels.
 */
export function dragSizes(start: DragStart, delta: number): DragResult {
  const { target } = start
  const from = target.isFolded ? (target.folded ?? 0) : target.size
  let wanted = from + delta
  if (wanted < target.min - EPSILON) {
    if (target.folded !== null && wanted < (target.min + target.folded) / 2) {
      return { size: null, folded: true }
    }
    wanted = target.min
  }
  wanted = Math.min(wanted, from + start.fillRoom)
  // Too little room to open the panel at its smallest size: it stays folded.
  if (wanted < target.min - EPSILON) {
    return { size: null, folded: true }
  }
  return { size: wanted, folded: false }
}

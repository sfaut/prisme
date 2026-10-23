/**
 * Size of the panels of the layout, set with the mouse (`PanelResizer.vue`)
 * and remembered from one visit to the next: the width of the side panels,
 * the object explorer on the left and the data explorer on the right, and
 * the height of the console, at the bottom.
 *
 * The size is kept in the browser's local storage: it is a display
 * preference specific to this machine. If storage is unavailable (private
 * browsing, blocked storage), the default size applies.
 */

/**
 * Bounds and memory of a panel's size: its width for a side panel, its
 * height for the console.
 */
export interface PanelSize {
  /**
   * Key of the size in local storage.
   */
  storageKey: string
  /**
   * Default size, in pixels.
   */
  defaultSize: number
  /**
   * Minimum size, in pixels.
   */
  minSize: number
  /**
   * Maximum share of the window taken by the panel, in the same direction.
   */
  maxRatio: number
}

/**
 * Left panel: object navigator. At least enough to read the connections'
 * buttons.
 */
export const SIDER: PanelSize = {
  storageKey: 'prisme.siderWidth',
  defaultSize: 300,
  minSize: 200,
  maxRatio: 0.7,
}

/**
 * Right panel: data explorer. At least enough for its buttons; at most half
 * of the window, to leave room for the grid.
 */
export const DATA_PANEL: PanelSize = {
  storageKey: 'prisme.dataPanelWidth',
  defaultSize: 420,
  minSize: 240,
  maxRatio: 0.5,
}

/**
 * Console, at the bottom: a few lines at least; at most two thirds of the
 * window, to leave room for the editor.
 */
export const CONSOLE: PanelSize = {
  storageKey: 'prisme.consoleHeight',
  defaultSize: 220,
  minSize: 120,
  maxRatio: 0.66,
}

/**
 * Brings a size back between the minimum and the maximum share of the window.
 *
 * @param panel Panel.
 * @param size Wanted size, in pixels.
 * @param windowSize Size of the window in the same direction, in pixels.
 */
export function clampPanelSize(panel: PanelSize, size: number, windowSize: number): number {
  const max = Math.max(panel.minSize, Math.floor(windowSize * panel.maxRatio))
  return Math.min(max, Math.max(panel.minSize, Math.round(size)))
}

/**
 * Remembered size, or the default size if none is readable.
 *
 * @param panel Panel.
 */
export function loadPanelSize(panel: PanelSize): number {
  try {
    const size = Number(window.localStorage.getItem(panel.storageKey))
    return Number.isFinite(size) && size > 0 ? size : panel.defaultSize
  } catch {
    return panel.defaultSize
  }
}

/**
 * Remembers the size; no effect if storage is unavailable.
 *
 * @param panel Panel.
 * @param size Size, in pixels.
 */
export function savePanelSize(panel: PanelSize, size: number): void {
  try {
    window.localStorage.setItem(panel.storageKey, String(size))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

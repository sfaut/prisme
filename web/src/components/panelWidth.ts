/**
 * Width of the side panels, set with the mouse (`PanelResizer.vue`) and
 * remembered from one visit to the next: on the left, the connections and
 * the explorer; on the right, the data explorer.
 *
 * The width is kept in the browser's local storage: it is a display
 * preference specific to this machine. If storage is unavailable (private
 * browsing, blocked storage), the default width applies.
 */

/**
 * Bounds and memory of a panel's width.
 */
export interface PanelWidth {
  /**
   * Key of the width in local storage.
   */
  storageKey: string
  /**
   * Default width, in pixels.
   */
  defaultWidth: number
  /**
   * Minimum width, in pixels.
   */
  minWidth: number
  /**
   * Maximum share of the window taken by the panel.
   */
  maxRatio: number
}

/**
 * Left panel: connections and explorer. At least enough to read the
 * connections' buttons.
 */
export const SIDER: PanelWidth = {
  storageKey: 'prisme.siderWidth',
  defaultWidth: 300,
  minWidth: 200,
  maxRatio: 0.7,
}

/**
 * Right panel: data explorer. At least enough for its buttons; at most half
 * of the window, to leave room for the grid.
 */
export const DATA_PANEL: PanelWidth = {
  storageKey: 'prisme.dataPanelWidth',
  defaultWidth: 420,
  minWidth: 240,
  maxRatio: 0.5,
}

/**
 * Brings a width back between the minimum and the maximum share of the window.
 *
 * @param panel Panel.
 * @param width Wanted width, in pixels.
 * @param windowWidth Width of the window, in pixels.
 */
export function clampPanelWidth(panel: PanelWidth, width: number, windowWidth: number): number {
  const max = Math.max(panel.minWidth, Math.floor(windowWidth * panel.maxRatio))
  return Math.min(max, Math.max(panel.minWidth, Math.round(width)))
}

/**
 * Remembered width, or the default width if none is readable.
 *
 * @param panel Panel.
 */
export function loadPanelWidth(panel: PanelWidth): number {
  try {
    const width = Number(window.localStorage.getItem(panel.storageKey))
    return Number.isFinite(width) && width > 0 ? width : panel.defaultWidth
  } catch {
    return panel.defaultWidth
  }
}

/**
 * Remembers the width; no effect if storage is unavailable.
 *
 * @param panel Panel.
 * @param width Width, in pixels.
 */
export function savePanelWidth(panel: PanelWidth, width: number): void {
  try {
    window.localStorage.setItem(panel.storageKey, String(width))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

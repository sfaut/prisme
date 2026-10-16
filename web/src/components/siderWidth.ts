/**
 * Width of the left panel (connections and explorer), set with the mouse and
 * remembered from one visit to the next.
 *
 * The width is kept in the browser's local storage: it is a display
 * preference specific to this machine. If storage is unavailable (private
 * browsing, blocked storage), the default width applies.
 */

/**
 * Default width, in pixels.
 */
export const DEFAULT_SIDER_WIDTH = 300

/**
 * Minimum width, in pixels: enough to read the connections' buttons.
 */
export const MIN_SIDER_WIDTH = 200

/**
 * Maximum share of the window taken by the panel.
 */
export const MAX_SIDER_RATIO = 0.7

/**
 * Key of the width in local storage.
 */
const STORAGE_KEY = 'prisme.siderWidth'

/**
 * Brings a width back between the minimum and the maximum share of the window.
 *
 * @param width Wanted width, in pixels.
 * @param windowWidth Width of the window, in pixels.
 */
export function clampSiderWidth(width: number, windowWidth: number): number {
  const max = Math.max(MIN_SIDER_WIDTH, Math.floor(windowWidth * MAX_SIDER_RATIO))
  return Math.min(max, Math.max(MIN_SIDER_WIDTH, Math.round(width)))
}

/**
 * Remembered width, or the default width if none is readable.
 */
export function loadSiderWidth(): number {
  try {
    const width = Number(window.localStorage.getItem(STORAGE_KEY))
    return Number.isFinite(width) && width > 0 ? width : DEFAULT_SIDER_WIDTH
  } catch {
    return DEFAULT_SIDER_WIDTH
  }
}

/**
 * Remembers the width; no effect if storage is unavailable.
 *
 * @param width Width, in pixels.
 */
export function saveSiderWidth(width: number): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, String(width))
  } catch {
    // Display preference only: losing it harms nothing.
  }
}

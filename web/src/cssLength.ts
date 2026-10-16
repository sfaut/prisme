/**
 * Reading a theme length (`main.css`) in pixels.
 *
 * The theme writes its lengths in `rem`, relative to the page's text size.
 * A few tools want a number of pixels though: virtual scrolling (height of a
 * row), the positioning of Reka UI's menus (gap with their button). They read
 * the variable here: the theme stays the only source of these lengths.
 */

/**
 * Value in pixels of a CSS variable of the theme, defined in `rem` or `px`.
 *
 * @param name Name of the variable (`--row-height`).
 * @param fallback Value returned if the variable is missing or unreadable
 *     (tests without a stylesheet, for example).
 */
export function cssPixels(name: string, fallback: number): number {
  const style = getComputedStyle(document.documentElement)
  const value = style.getPropertyValue(name).trim()
  const number = Number.parseFloat(value)
  if (Number.isNaN(number)) {
    return fallback
  }
  if (value.endsWith('rem')) {
    return number * (Number.parseFloat(style.fontSize) || 16)
  }
  return value.endsWith('px') ? number : fallback
}

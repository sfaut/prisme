/**
 * Guard on the radii of the interface: the components only use the three
 * roles of the theme (`--radius-tab`, `--radius-control`, `--radius-overlay`
 * in `assets/main.css`, `rounded-tab`, `rounded-control`, `rounded-overlay`
 * in the classes) and the round shapes of the pills and circles. A step of
 * Tailwind's scale written in a component would escape the theme: changing
 * a role would leave it behind.
 */
import { describe, expect, it } from 'vitest'

/**
 * Source of every component, script and stylesheet, by path, tests left out.
 */
const SOURCES = import.meta.glob(
  ['../**/*.vue', '../**/*.ts', '../assets/*.css', '!../**/__tests__/**'],
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>

/**
 * Radii taken from Tailwind's scale instead of a role: a class with a step
 * or an arbitrary value (`rounded-lg`, `rounded-e-xl`, `rounded-[4px]`), a
 * variable of the scale (`var(--radius-lg)`).
 */
const SCALE_RADII = [
  /(?<![\w-])rounded(?:-(?:[setblrxy]|ss|se|es|ee|tl|tr|bl|br))?-(?:xs|sm|md|lg|xl|[234]xl|\[[^\]]*\]|\([^)]*\))(?![\w-])/,
  /--radius-(?:xs|sm|md|lg|xl|[234]xl)(?![\w-])/,
]

/**
 * Declaration of a role in the theme, the only place where the scale is read.
 */
const ROLE_DECLARATION = /^--radius-(?:tab|control|overlay)\s*:/

/**
 * Value of a `border-radius` written in a style: a role, or a round shape
 * (pill as `rounded-full`, circle, ends of a splitter's grip).
 */
const RADIUS_VALUE = /(?:border-radius\s*:|borderRadius\s*:)\s*['"]?([^;'",]+)/
const ALLOWED_VALUES = [
  /^var\(--radius-(?:tab|control|overlay)\)$/,
  /^calc\(infinity \* 1px\)$/,
  /^50%$/,
  /^var\(--grip-thickness\)$/,
]

/**
 * Lines of the sources that break a rule, with their path and number.
 */
function offenders(breaks: (line: string) => boolean) {
  return Object.entries(SOURCES).flatMap(([path, source]) =>
    source
      .split('\n')
      .map((line, index) => ({ path, number: index + 1, line: line.trim() }))
      .filter(({ line }) => breaks(line)),
  )
}

describe('radii', () => {
  it('are found in the sources', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(10)
  })

  it('come from the roles of the theme, never from the scale', () => {
    expect(
      offenders(
        (line) => !ROLE_DECLARATION.test(line) && SCALE_RADII.some((pattern) => pattern.test(line)),
      ),
    ).toEqual([])
  })

  it('are written in the styles with a role or a round shape', () => {
    expect(
      offenders((line) => {
        const value = RADIUS_VALUE.exec(line)?.[1]?.trim()
        return value !== undefined && !ALLOWED_VALUES.some((pattern) => pattern.test(value))
      }),
    ).toEqual([])
  })
})

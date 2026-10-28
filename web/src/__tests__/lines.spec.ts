/**
 * Guard on the lines of the interface: a line is drawn by a border (or an
 * outline), never by a box one line wide filled by its background. The
 * browser rounds the width of a border to whole pixels of the screen; a box
 * of one CSS pixel covers one and a half at a scale of 150% and is painted
 * one or two pixels thick according to its position (`docs/DEVELOPER.md`,
 * section "Lines").
 */
import { describe, expect, it } from 'vitest'

/**
 * Source of every component and of the stylesheet, by path.
 */
const SOURCES = import.meta.glob(['../**/*.vue', '../assets/*.css'], {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

/**
 * Ways of sizing a box to the thickness of a line: a width or a height of
 * `--line-width` or of one pixel, in a `<style>` block or in a class.
 */
const LINE_BOXES = [
  /(?<![\w-])(?:inline-size|block-size|width|height)\s*:\s*(?:var\(--line-width\)|1px)\s*;/,
  /(?<![\w-])[wh]-(?:px|\(--line-width\)|\[1px\])(?![\w-])/,
]

describe('lines', () => {
  it('are found in the sources', () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(10)
  })

  it('are drawn by borders, never by a box one line wide', () => {
    const offenders = Object.entries(SOURCES).flatMap(([path, source]) =>
      source
        .split('\n')
        .map((line, index) => ({ path, number: index + 1, line: line.trim() }))
        .filter(({ line }) => LINE_BOXES.some((pattern) => pattern.test(line))),
    )
    expect(offenders).toEqual([])
  })
})

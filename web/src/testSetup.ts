/**
 * Setup of the Vitest tests, loaded before each test file (`setupFiles` of
 * `vitest.config.ts`).
 *
 * jsdom computes no layout: its `Range` has neither `getClientRects` nor
 * `getBoundingClientRect`. CodeMirror uses them to place the selection and the
 * error underlines; without them, it writes a `TypeError` to the console,
 * depending on when it measures. Both methods return empty rectangles here,
 * like a text that is not displayed.
 *
 * Nor do its elements scroll: `Element.scrollTo`, which the console calls to
 * follow its most recent lines, does nothing here.
 */

if (typeof Range !== 'undefined') {
  Range.prototype.getClientRects ??= function (): DOMRectList {
    const rects: DOMRect[] = []
    return Object.assign(rects, { item: (index: number) => rects[index] ?? null })
  }
  Range.prototype.getBoundingClientRect ??= () => new DOMRect(0, 0, 0, 0)
}

if (typeof Element !== 'undefined') {
  Element.prototype.scrollTo ??= () => {}
}

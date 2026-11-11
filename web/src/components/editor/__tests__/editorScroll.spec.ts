import type { EditorView } from '@codemirror/view'
import { describe, expect, it } from 'vitest'

import { scrollKeymap, scrollLines } from '../editorScroll'

/**
 * Minimal editor view: the scroll area and the height of a line.
 *
 * @param scrollTop Initial scroll position, in pixels.
 */
function fakeView(scrollTop: number): EditorView {
  return { scrollDOM: { scrollTop }, defaultLineHeight: 20 } as unknown as EditorView
}

describe('scrolling with the keyboard', () => {
  it('scrolls by whole lines, up or down', () => {
    const view = fakeView(100)
    expect(scrollLines(1)(view)).toBe(true)
    expect(view.scrollDOM.scrollTop).toBe(120)
    scrollLines(-2)(view)
    expect(view.scrollDOM.scrollTop).toBe(80)
  })

  it('binds Ctrl+Up and Ctrl+Down', () => {
    expect(scrollKeymap.map((binding) => binding.key)).toEqual(['Ctrl-ArrowUp', 'Ctrl-ArrowDown'])
    const view = fakeView(100)
    scrollKeymap[0]!.run!(view)
    expect(view.scrollDOM.scrollTop).toBe(80)
  })
})

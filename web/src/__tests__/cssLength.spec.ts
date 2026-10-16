import { afterEach, describe, expect, it } from 'vitest'

import { cssPixels } from '../cssLength'

afterEach(() => {
  document.documentElement.removeAttribute('style')
})

describe('cssPixels', () => {
  it('converts a length in rem or px to pixels', () => {
    document.documentElement.style.fontSize = '16px'
    document.documentElement.style.setProperty('--hauteur', '1.5rem')
    document.documentElement.style.setProperty('--ecart', '4px')
    expect(cssPixels('--hauteur', 0)).toBe(24)
    expect(cssPixels('--ecart', 0)).toBe(4)
  })

  it('returns the fallback value for a missing or unreadable variable', () => {
    document.documentElement.style.setProperty('--illisible', 'auto')
    expect(cssPixels('--absente', 22)).toBe(22)
    expect(cssPixels('--illisible', 22)).toBe(22)
  })
})

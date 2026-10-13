import { afterEach, describe, expect, it } from 'vitest'

import { cssPixels } from '../cssLength'

afterEach(() => {
  document.documentElement.removeAttribute('style')
})

describe('cssPixels', () => {
  it('convertit en pixels une longueur en rem ou en px', () => {
    document.documentElement.style.fontSize = '16px'
    document.documentElement.style.setProperty('--hauteur', '1.5rem')
    document.documentElement.style.setProperty('--ecart', '4px')
    expect(cssPixels('--hauteur', 0)).toBe(24)
    expect(cssPixels('--ecart', 0)).toBe(4)
  })

  it('rend la valeur de repli pour une variable absente ou illisible', () => {
    document.documentElement.style.setProperty('--illisible', 'auto')
    expect(cssPixels('--absente', 22)).toBe(22)
    expect(cssPixels('--illisible', 22)).toBe(22)
  })
})

import { afterEach, describe, expect, it } from 'vitest'

import { loadFlag, loadSize, saveFlag, saveSize } from '../storedValues'

afterEach(() => {
  window.localStorage.clear()
})

describe('flags', () => {
  it('reads a saved flag, and the fallback without one', () => {
    expect(loadFlag('flag', true)).toBe(true)
    saveFlag('flag', false)
    expect(window.localStorage.getItem('flag')).toBe('false')
    expect(loadFlag('flag', true)).toBe(false)
  })
})

describe('sizes', () => {
  it('reads a saved size, unrounded, and forgets it with null', () => {
    saveSize('size', 227.67)
    expect(loadSize('size')).toBe(227.67)
    saveSize('size', null)
    expect(window.localStorage.getItem('size')).toBeNull()
    expect(loadSize('size')).toBeNull()
  })

  it('gives the default size for a value that is no size', () => {
    for (const value of ['', '-3', '0', 'wide', 'Infinity']) {
      window.localStorage.setItem('size', value)
      expect(loadSize('size')).toBeNull()
    }
  })
})

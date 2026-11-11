import { describe, expect, it } from 'vitest'

import { rememberWidths, rememberedWidths } from '../gridWidths'

describe('remembered widths', () => {
  it('come back for the same query, with the same columns', () => {
    rememberWidths('SELECT a, b FROM t', ['a', 'b'], { 1: '12.5rem' })
    expect(rememberedWidths('SELECT a, b FROM t', ['a', 'b'])).toEqual({ 1: '12.5rem' })
    expect(rememberedWidths('SELECT a FROM t', ['a'])).toEqual({})
    // Columns changed in the meantime: the estimated widths apply again.
    expect(rememberedWidths('SELECT a, b FROM t', ['a', 'c'])).toEqual({})
  })

  it('forget the oldest queries', () => {
    rememberWidths('query 0', ['a'], { 0: '5.625rem' })
    for (let index = 1; index <= 100; index++) {
      rememberWidths(`query ${index}`, ['a'], { 0: '5.625rem' })
    }
    expect(rememberedWidths('query 0', ['a'])).toEqual({})
    expect(rememberedWidths('query 100', ['a'])).toEqual({ 0: '5.625rem' })
  })
})

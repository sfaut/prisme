import { describe, expect, it } from 'vitest'

import { rememberWidths, rememberedWidths } from '../gridWidths'

describe('largeurs retenues', () => {
  it('reviennent pour la même requête, avec les mêmes colonnes', () => {
    rememberWidths('SELECT a, b FROM t', ['a', 'b'], { 1: 200 })
    expect(rememberedWidths('SELECT a, b FROM t', ['a', 'b'])).toEqual({ 1: 200 })
    expect(rememberedWidths('SELECT a FROM t', ['a'])).toEqual({})
    // Colonnes changées entre-temps : on repart des largeurs estimées.
    expect(rememberedWidths('SELECT a, b FROM t', ['a', 'c'])).toEqual({})
  })

  it('oublient les requêtes les plus anciennes', () => {
    rememberWidths('requête 0', ['a'], { 0: 90 })
    for (let index = 1; index <= 100; index++) {
      rememberWidths(`requête ${index}`, ['a'], { 0: 90 })
    }
    expect(rememberedWidths('requête 0', ['a'])).toEqual({})
    expect(rememberedWidths('requête 100', ['a'])).toEqual({ 0: 90 })
  })
})

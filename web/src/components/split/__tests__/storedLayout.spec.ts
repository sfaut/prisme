import { afterEach, describe, expect, it } from 'vitest'
import { nextTick } from 'vue'

import { forgetStoredLayout, useStoredLayout } from '../storedLayout'

afterEach(() => {
  window.localStorage.clear()
})

describe('useStoredLayout', () => {
  it('reads the values of the right kind, and gives the others their default', () => {
    window.localStorage.setItem(
      'layout',
      JSON.stringify({ editor: 210.5, results: -3, pivot: 'high', folded: true, open: 1 }),
    )
    const layout = useStoredLayout('layout', {
      editor: null as number | null,
      results: null as number | null,
      pivot: null as number | null,
      folded: false as boolean,
      open: true as boolean,
    })
    expect({ ...layout }).toEqual({
      editor: 210.5,
      results: null,
      pivot: null,
      folded: true,
      open: true,
    })
  })

  it('saves each change, which forgetting the layout removes', async () => {
    const layout = useStoredLayout('layout', { size: null as number | null })
    layout.size = 331.25
    await nextTick()
    expect(JSON.parse(window.localStorage.getItem('layout')!)).toEqual({ size: 331.25 })
    forgetStoredLayout('layout')
    expect(window.localStorage.getItem('layout')).toBeNull()
  })

  it('keeps its start at once on request, when nothing was kept', () => {
    useStoredLayout('started', { size: 120 as number | null }, true)
    expect(JSON.parse(window.localStorage.getItem('started')!)).toEqual({ size: 120 })
    // A layout already kept stays as it is.
    useStoredLayout('started', { size: 300 as number | null }, true)
    expect(JSON.parse(window.localStorage.getItem('started')!)).toEqual({ size: 120 })
    useStoredLayout('unkept', { size: null as number | null })
    expect(window.localStorage.getItem('unkept')).toBeNull()
    forgetStoredLayout('started')
  })
})

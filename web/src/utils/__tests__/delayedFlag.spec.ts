import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { effectScope, nextTick, ref } from 'vue'

import { delayedFlag } from '../delayedFlag'

beforeEach(() => {
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('delayedFlag', () => {
  it('follows its source once it has stayed true for the delay', async () => {
    const scope = effectScope()
    const source = ref(false)
    const flag = scope.run(() => delayedFlag(source, 500))!
    source.value = true
    await nextTick()
    vi.advanceTimersByTime(499)
    expect(flag.value).toBe(false)
    vi.advanceTimersByTime(1)
    expect(flag.value).toBe(true)
    source.value = false
    await nextTick()
    expect(flag.value).toBe(false)
    scope.stop()
  })

  it('stays false for a source true for less than the delay, again and again', async () => {
    const scope = effectScope()
    const source = ref(false)
    const flag = scope.run(() => delayedFlag(source, 500))!
    for (let time = 0; time < 5; time++) {
      source.value = true
      await nextTick()
      vi.advanceTimersByTime(300)
      source.value = false
      await nextTick()
      vi.advanceTimersByTime(700)
      expect(flag.value).toBe(false)
    }
    scope.stop()
  })

  it('stops its timer with its scope', async () => {
    const scope = effectScope()
    const source = ref(true)
    const flag = scope.run(() => delayedFlag(source, 500))!
    scope.stop()
    vi.advanceTimersByTime(500)
    expect(flag.value).toBe(false)
    expect(vi.getTimerCount()).toBe(0)
  })
})
